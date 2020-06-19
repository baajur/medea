//! Room definitions and implementations. Room is responsible for media
//! connection establishment between concrete [`Member`]s.

mod command_handler;
mod dynamic_api;
mod peer_events_handler;
mod rpc_server;

use std::sync::Arc;

use actix::{
    Actor, ActorFuture, AsyncContext as _, Context, Handler, MailboxError,
    WrapFuture as _,
};
use derive_more::{Display, From};
use failure::Fail;
use medea_client_api_proto::{Event, NegotiationRole, PeerId};

use crate::{
    api::control::{
        callback::{
            clients::CallbackClientFactoryImpl, service::CallbackService,
            OnLeaveEvent, OnLeaveReason,
        },
        refs::{Fid, StatefulFid, ToEndpoint, ToMember},
        room::RoomSpec,
        MemberId, RoomId,
    },
    log::prelude::*,
    media::{Peer, PeerError, Stable},
    shutdown::ShutdownGracefully,
    signalling::{
        elements::{member::MemberError, Member, MembersLoadError},
        participants::{ParticipantService, ParticipantServiceErr},
        peers::{ConnectEndpointsResult, PeerTrafficWatcher, PeersService},
    },
    turn::TurnServiceErr,
    utils::{actix_try_join_all, ResponseActAnyFuture},
    AppContext,
};

pub use dynamic_api::{
    Close, CreateEndpoint, CreateMember, Delete, SerializeProto,
};
use std::collections::HashSet;

/// Ergonomic type alias for using [`ActorFuture`] for [`Room`].
pub type ActFuture<O> = Box<dyn ActorFuture<Actor = Room, Output = O>>;

#[derive(Debug, Display, Fail, From)]
pub enum RoomError {
    #[display(fmt = "Couldn't find Peer with [id = {}]", _0)]
    PeerNotFound(PeerId),

    MemberError(MemberError),

    #[display(fmt = "Member [id = {}] does not have Turn credentials", _0)]
    #[from(ignore)]
    NoTurnCredentials(MemberId),

    #[display(fmt = "Couldn't find RpcConnection with Member [id = {}]", _0)]
    #[from(ignore)]
    ConnectionNotExists(MemberId),

    #[display(fmt = "Unable to send event to Member [id = {}]", _0)]
    #[from(ignore)]
    UnableToSendEvent(MemberId),

    #[display(fmt = "PeerError: {}", _0)]
    PeerError(PeerError),

    #[display(fmt = "{}", _0)]
    MembersLoadError(MembersLoadError),

    #[display(fmt = "Generic room error: {}", _0)]
    #[from(ignore)]
    BadRoomSpec(String),

    ParticipantServiceErr(ParticipantServiceErr),

    #[display(fmt = "Given Fid [fid = {}] to wrong Room [id = {}]", _0, _1)]
    WrongRoomId(StatefulFid, RoomId),

    /// Try to create [`Member`] with ID which already exists.
    #[display(fmt = "Member [id = {}] already exists.", _0)]
    MemberAlreadyExists(Fid<ToMember>),

    /// Try to create [`Endpoint`] with ID which already exists.
    ///
    /// [`Endpoint`]: crate::signalling::elements::endpoints::Endpoint
    #[display(fmt = "Endpoint [id = {}] already exists.", _0)]
    EndpointAlreadyExists(Fid<ToEndpoint>),

    /// [`TurnAuthService`] errored to perform an operation.
    ///
    /// [`TurnAuthService`]: crate::turn::service::TurnAuthService
    #[display(fmt = "TurnService errored in Room: {}", _0)]
    TurnServiceErr(TurnServiceErr),

    /// [`MailboxError`] returned on sending message to [`PeerTrafficWatcher`]
    /// service.
    #[display(
        fmt = "Mailbox error while sending message to PeerTrafficWatcher \
               service: {}",
        _0
    )]
    #[from(ignore)]
    PeerTrafficWatcherMailbox(MailboxError),
}

/// Possible states of [`Room`].
#[derive(Debug)]
enum State {
    /// [`Room`] has been started and is operating at the moment.
    Started,
    /// [`Room`] is stopping at the moment.
    Stopping,
    /// [`Room`] is stopped and can be removed.
    Stopped,
}

/// Media server room with its [`Member`]s.
#[derive(Debug)]
pub struct Room {
    id: RoomId,

    /// Service for sending [`CallbackEvent`]s.
    ///
    /// [`CallbackEvent`]: crate::api::control::callback::CallbackEvent
    callbacks: CallbackService<CallbackClientFactoryImpl>,

    /// [`Member`]s and associated [`RpcConnection`]s of this [`Room`], handles
    /// [`RpcConnection`] authorization, establishment, message sending.
    ///
    /// [`RpcConnection`]: crate::api::client::rpc_connection::RpcConnection
    members: ParticipantService,

    /// [`Peer`]s of [`Member`]s in this [`Room`].
    peers: PeersService,

    /// Current state of this [`Room`].
    state: State,
}

impl Room {
    /// Creates new instance of [`Room`].
    ///
    /// # Errors
    ///
    /// Errors with [`RoomError::BadRoomSpec`] if [`RoomSpec`] transformation
    /// fails.
    pub fn new(
        room_spec: &RoomSpec,
        context: &AppContext,
        peers_traffic_watcher: Arc<dyn PeerTrafficWatcher>,
    ) -> Result<Self, RoomError> {
        Ok(Self {
            id: room_spec.id().clone(),
            peers: PeersService::new(
                room_spec.id().clone(),
                context.turn_service.clone(),
                peers_traffic_watcher,
                &context.config.media,
            ),
            members: ParticipantService::new(room_spec, context)?,
            state: State::Started,
            callbacks: context.callbacks.clone(),
        })
    }

    /// Returns [`RoomId`] of this [`Room`].
    pub fn id(&self) -> &RoomId {
        &self.id
    }

    /// Sends [`Event::PeerCreated`] specified [`Peer`]. That [`Peer`] state
    /// will be changed to [`WaitLocalSdp`] state.
    fn send_peer_created(
        &mut self,
        peer_id: PeerId,
    ) -> Result<ActFuture<Result<(), RoomError>>, RoomError> {
        let peer: Peer<Stable> = self.peers.take_inner_peer(peer_id)?;

        let peer = peer.start();
        let member_id = peer.member_id();
        let ice_servers = peer
            .ice_servers_list()
            .ok_or_else(|| RoomError::NoTurnCredentials(member_id.clone()))?;
        let peer_created = Event::PeerCreated {
            peer_id: peer.id(),
            negotiation_role: NegotiationRole::Offerer,
            tracks: peer.new_tracks(),
            ice_servers,
            force_relay: peer.is_force_relayed(),
        };
        self.peers.add_peer(peer);
        Ok(Box::new(
            self.members
                .send_event_to_member(member_id, peer_created)
                .into_actor(self),
        ))
    }

    /// Sends [`Event::PeersRemoved`] to [`Member`].
    fn send_peers_removed(
        &mut self,
        member_id: MemberId,
        removed_peers_ids: Vec<PeerId>,
    ) -> ActFuture<Result<(), RoomError>> {
        Box::new(
            self.members
                .send_event_to_member(
                    member_id,
                    Event::PeersRemoved {
                        peer_ids: removed_peers_ids,
                    },
                )
                .into_actor(self),
        )
    }

    /// Connects interconnected [`Endpoint`]s between provided [`Member`]s.
    fn connect_members(
        &mut self,
        member1: &Member,
        member2: &Member,
    ) -> ActFuture<Result<(), RoomError>> {
        let member2_id = member2.id();
        let mut connect_endpoints_tasks = Vec::new();

        for src in member1.srcs().values() {
            for sink in src.sinks() {
                if sink.owner().id() == member2_id {
                    connect_endpoints_tasks.push(
                        self.peers.clone().connect_endpoints(src.clone(), sink),
                    );
                }
            }
        }

        for sink in member1.sinks().values() {
            let src = sink.src();
            if src.owner().id() == member2_id {
                connect_endpoints_tasks.push(
                    self.peers.clone().connect_endpoints(src, sink.clone()),
                )
            }
        }

        Box::new(
            futures::future::try_join_all(connect_endpoints_tasks)
                .into_actor(self)
                .then(move |result, room: &mut Room, ctx| {
                    let connected_peers = actix_try!(result);

                    let mut spawn_futs = Vec::new();
                    let mut then_futs = Vec::new();
                    let mut known_peer_ids = HashSet::new();

                    for connected_peer in connected_peers {
                        match connected_peer {
                            ConnectEndpointsResult::NoOp(_, _) => {
                                // Just do nothing
                            }
                            ConnectEndpointsResult::Updated(
                                first_peer_id,
                                second_peer_id,
                            ) => {
                                if !known_peer_ids.contains(&first_peer_id) {
                                    let first_peer: Peer<Stable> =
                                        actix_try!(room
                                            .peers
                                            .take_inner_peer(first_peer_id));
                                    let first_peer =
                                        first_peer.start_renegotiation();
                                    let event = Event::TracksApplied {
                                        updates: first_peer.get_updates(),
                                        negotiation_role: Some(
                                            NegotiationRole::Offerer,
                                        ),
                                        peer_id: first_peer_id,
                                    };
                                    then_futs.push(Box::new(
                                        room.members
                                            .send_event_to_member(
                                                first_peer.member_id(),
                                                event,
                                            )
                                            .into_actor(room),
                                    )
                                        as ActFuture<_>);

                                    room.peers.add_peer(first_peer);

                                    known_peer_ids.insert(first_peer_id);
                                    known_peer_ids.insert(second_peer_id);
                                }
                            }
                            ConnectEndpointsResult::Created(
                                first_peer_id,
                                second_peer_id,
                            ) => {
                                if !known_peer_ids.contains(&first_peer_id) {
                                    spawn_futs
                                        .push(actix_try!(room
                                            .send_peer_created(first_peer_id)));
                                    known_peer_ids.insert(first_peer_id);
                                    known_peer_ids.insert(second_peer_id);
                                }
                            }
                        }
                    }

                    ctx.spawn(actix_try_join_all(spawn_futs).map(
                        |res, room, ctx| {
                            if let Err(e) = res {
                                error!(
                                    "Failed to connect Endpoints because: {:?}",
                                    e
                                );
                                room.close_gracefully(ctx);
                            }
                        },
                    ));

                    Box::new(actix_try_join_all(then_futs).map(|res, _, _| {
                        res?;
                        Ok(())
                    }))
                }),
        )
    }

    /// Creates and interconnects all [`Peer`]s between connected [`Member`]
    /// and all available at this moment other [`Member`]s. Expects that
    /// provided [`Member`] have active [`RpcConnection`].
    ///
    /// Availability is determined by checking [`RpcConnection`] of all
    /// [`Member`]s from [`WebRtcPlayEndpoint`]s and from receivers of
    /// the connected [`Member`].
    ///
    /// Will start renegotiation with `MediaTrack`s adding if some not
    /// interconnected `Endpoint`s will be found and if [`Peer`]s pair is
    /// already exists.
    fn init_member_connections(
        &mut self,
        member: &Member,
    ) -> ActFuture<Result<(), RoomError>> {
        let connect_members_tasks =
            member.partners().into_iter().filter_map(|partner| {
                if self.members.member_has_connection(&partner.id()) {
                    Some(self.connect_members(&partner, member))
                } else {
                    None
                }
            });

        Box::new(
            actix_try_join_all(connect_members_tasks)
                .map(|result, _, _| result.map(|_| ())),
        )
    }

    /// Closes [`Room`] gracefully, by dropping all the connections and moving
    /// into [`State::Stopped`].
    fn close_gracefully(&mut self, ctx: &mut Context<Self>) -> ActFuture<()> {
        info!("Closing Room [id = {}]", self.id);
        self.state = State::Stopping;

        self.members
            .iter_members()
            .filter_map(|(_, member)| {
                member
                    .get_on_leave()
                    .map(move |on_leave| (member, on_leave))
            })
            .filter(|(member, _)| {
                self.members.member_has_connection(&member.id())
            })
            .for_each(|(member, on_leave)| {
                self.callbacks.send_callback(
                    on_leave,
                    member.get_fid().into(),
                    OnLeaveEvent::new(OnLeaveReason::ServerShutdown),
                );
            });

        Box::new(self.members.drop_connections(ctx).into_actor(self).map(
            |_, room: &mut Self, _| {
                room.state = State::Stopped;
            },
        ))
    }

    /// Signals about removing [`Member`]'s [`Peer`]s.
    fn member_peers_removed(
        &mut self,
        peers_id: Vec<PeerId>,
        member_id: MemberId,
        ctx: &mut Context<Self>,
    ) -> ActFuture<()> {
        info!(
            "Peers {:?} removed for member [id = {}].",
            peers_id, member_id
        );
        if let Some(member) = self.members.get_member_by_id(&member_id) {
            member.peers_removed(&peers_id);
        } else {
            error!(
                "Member [id = {}] for which received Event::PeersRemoved not \
                 found. Closing room.",
                member_id
            );

            return self.close_gracefully(ctx);
        }

        Box::new(self.send_peers_removed(member_id, peers_id).then(
            |err, this, ctx: &mut Context<Self>| {
                if let Err(e) = err {
                    match e {
                        RoomError::ConnectionNotExists(_)
                        | RoomError::UnableToSendEvent(_) => {
                            Box::new(actix::fut::ready(()))
                        }
                        _ => {
                            error!(
                                "Unexpected failed PeersEvent command, \
                                 because {}. Room will be stopped.",
                                e
                            );
                            this.close_gracefully(ctx)
                        }
                    }
                } else {
                    Box::new(actix::fut::ready(()))
                }
            },
        ))
    }
}

/// [`Actor`] implementation that provides an ergonomic way
/// to interact with [`Room`].
impl Actor for Room {
    type Context = Context<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        debug!("Room [id = {}] started.", self.id);
    }
}

impl Handler<ShutdownGracefully> for Room {
    type Result = ResponseActAnyFuture<Self, ()>;

    fn handle(
        &mut self,
        _: ShutdownGracefully,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        info!(
            "Room [id = {}] received ShutdownGracefully message so shutting \
             down",
            self.id
        );
        ResponseActAnyFuture(self.close_gracefully(ctx))
    }
}
