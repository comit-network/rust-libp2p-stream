use anyhow::Context as _;
use anyhow::Result;
use asynchronous_codec::Bytes;
use futures::{SinkExt, StreamExt};
use libp2p_xtra::libp2p::identity::Keypair;
use libp2p_xtra::libp2p::transport::MemoryTransport;
use libp2p_xtra::libp2p::PeerId;
use libp2p_xtra::{
    Connect, Disconnect, GetConnectionStats, ListenOn, NewInboundSubstream, Node, OpenSubstream,
};
use std::collections::HashSet;
use std::time::Duration;
use tokio_tasks::Tasks;
use xtra::message_channel::StrongMessageChannel;
use xtra::spawn::TokioGlobalSpawnExt;
use xtra::{Actor, Address};
use xtra_productivity::xtra_productivity;

#[tokio::test]
async fn hello_world() {
    let alice_hello_world_handler = HelloWorld::default().create(None).spawn_global();
    let (alice_peer_id, _, _alice, bob) = alice_and_bob(
        [(
            "/hello-world/1.0.0",
            alice_hello_world_handler.clone_channel(),
        )],
        [],
    )
    .await;

    let bob_to_alice = bob
        .send(OpenSubstream {
            peer: alice_peer_id,
            protocol: "/hello-world/1.0.0",
        })
        .await
        .unwrap()
        .unwrap();

    let string = hello_world_dialer(bob_to_alice, "Bob").await.unwrap();

    assert_eq!(string, "Hello Bob!");
}

#[tokio::test]
async fn after_connect_see_each_other_as_connected() {
    let (alice_peer_id, bob_peer_id, alice, bob) = alice_and_bob([], []).await;

    let alice_stats = alice.send(GetConnectionStats).await.unwrap();
    let bob_stats = bob.send(GetConnectionStats).await.unwrap();

    assert_eq!(alice_stats.connected_peers, HashSet::from([bob_peer_id]));
    assert_eq!(bob_stats.connected_peers, HashSet::from([alice_peer_id]));
}

#[tokio::test]
async fn disconnect_is_reflected_in_stats() {
    let (_, bob_peer_id, alice, bob) = alice_and_bob([], []).await;

    alice.send(Disconnect(bob_peer_id)).await.unwrap();

    let alice_stats = alice.send(GetConnectionStats).await.unwrap();
    let bob_stats = bob.send(GetConnectionStats).await.unwrap();

    assert_eq!(alice_stats.connected_peers, HashSet::from([]));
    assert_eq!(bob_stats.connected_peers, HashSet::from([]));
}

#[tokio::test]
async fn cannot_open_substream_for_unhandled_protocol() {
    let (_, bob_peer_id, alice, _bob) = alice_and_bob([], []).await;

    let error = alice
        .send(OpenSubstream {
            peer: bob_peer_id,
            protocol: "/foo/bar/1.0.0",
        })
        .await
        .unwrap()
        .unwrap_err();

    assert!(matches!(
        error,
        libp2p_xtra::Error::NegotiationFailed(libp2p_xtra::NegotiationError::Failed)
    ))
}

#[tokio::test]
async fn cannot_connect_twice() {
    assert!(false)
}

#[tokio::test]
async fn can_request_two_protocols() {
    assert!(false)
}

#[tokio::test]
async fn connection_non_listening_peer_times_out() {
    assert!(false)
}

async fn alice_and_bob<const AN: usize, const BN: usize>(
    alice_inbound_substream_handlers: [(
        &'static str,
        Box<dyn StrongMessageChannel<NewInboundSubstream>>,
    ); AN],
    bob_inbound_substream_handlers: [(
        &'static str,
        Box<dyn StrongMessageChannel<NewInboundSubstream>>,
    ); BN],
) -> (PeerId, PeerId, Address<Node>, Address<Node>) {
    let port = rand::random::<u16>();

    let alice_id = Keypair::generate_ed25519();
    let alice_peer_id = alice_id.public().to_peer_id();
    let bob_id = Keypair::generate_ed25519();
    let bob_peer_id = bob_id.public().to_peer_id();

    let alice = Node::new(
        MemoryTransport::default(),
        alice_id.clone(),
        Duration::from_secs(20),
        alice_inbound_substream_handlers,
    )
    .create(None)
    .spawn_global();
    let bob = Node::new(
        MemoryTransport::default(),
        bob_id.clone(),
        Duration::from_secs(20),
        bob_inbound_substream_handlers,
    )
    .create(None)
    .spawn_global();

    alice
        .send(ListenOn(format!("/memory/{port}").parse().unwrap()))
        .await
        .unwrap();

    bob.send(Connect(
        format!("/memory/{port}/p2p/{alice_peer_id}")
            .parse()
            .unwrap(),
    ))
    .await
    .unwrap()
    .unwrap();

    (alice_peer_id, bob_peer_id, alice, bob)
}

#[derive(Default)]
struct HelloWorld {
    tasks: Tasks,
}

#[xtra_productivity(message_impl = false)]
impl HelloWorld {
    async fn handle(&mut self, msg: NewInboundSubstream) {
        tracing::info!("New hello world stream from {}", msg.peer);

        self.tasks
            .add_fallible(hello_world_listener(msg.stream), move |e| async move {
                tracing::warn!("Hello world protocol with peer {} failed: {}", msg.peer, e);
            });
    }
}

impl xtra::Actor for HelloWorld {}

async fn hello_world_dialer(stream: libp2p_xtra::Substream, name: &'static str) -> Result<String> {
    let mut stream = asynchronous_codec::Framed::new(stream, asynchronous_codec::LengthCodec);

    stream.send(Bytes::from(name)).await?;
    let bytes = stream.next().await.context("Expected message")??;
    let message = String::from_utf8(bytes.to_vec())?;

    Ok(message)
}

async fn hello_world_listener(stream: libp2p_xtra::Substream) -> Result<()> {
    let mut stream =
        asynchronous_codec::Framed::new(stream, asynchronous_codec::LengthCodec).fuse();

    let bytes = stream.select_next_some().await?;
    let name = String::from_utf8(bytes.to_vec())?;

    stream.send(Bytes::from(format!("Hello {name}!"))).await?;

    Ok(())
}
