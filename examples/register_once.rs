use anyhow::Result;
use libp2p::core::identity::ed25519::SecretKey;
use libp2p::dns::TokioDnsConfig;
use libp2p::futures::StreamExt;
use libp2p::rendezvous::{Config, Namespace, Rendezvous};
use libp2p::swarm::{AddressScore, SwarmBuilder, SwarmEvent};
use libp2p::tcp::TokioTcpConfig;
use libp2p::{identity, rendezvous, Multiaddr, PeerId, Transport};
use rendezvous_server::transport::authenticate_and_multiplex;
use rendezvous_server::{parse_secret_key, Behaviour, Event};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Cli {
    #[structopt(long = "rendezvous-peer_id")]
    pub rendezvous_peer_id: PeerId,
    #[structopt(long = "rendezvous-addr")]
    pub rendezvous_addr: Multiaddr,
    #[structopt(
        long = "external-addr",
        help = "A public facing address is registered with the rendezvous server"
    )]
    pub external_addr: Multiaddr,
    #[structopt(long = "secret-key", parse(try_from_str = parse_secret_key))]
    pub secret_key: SecretKey,
    #[structopt(long = "port", help = "Listen port")]
    pub port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::from_args();

    let identity = identity::Keypair::generate_ed25519();

    let rendezvous_point_address = cli.rendezvous_addr;
    let rendezvous_point = cli.rendezvous_peer_id;

    let tcp_with_dns = TokioDnsConfig::system(TokioTcpConfig::new().nodelay(true)).unwrap();

    let transport = authenticate_and_multiplex(tcp_with_dns.boxed(), &identity).unwrap();

    let rendezvous = Rendezvous::new(identity.clone(), Config::default());

    let peer_id = PeerId::from(identity.public());

    let mut swarm = SwarmBuilder::new(transport, Behaviour::new(rendezvous), peer_id)
        .executor(Box::new(|f| {
            tokio::spawn(f);
        }))
        .build();

    println!("Local peer id: {}", swarm.local_peer_id());

    let _ = swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", cli.port).parse().unwrap());

    let _ = swarm.add_external_address(cli.external_addr, AddressScore::Infinite);

    swarm.dial_addr(rendezvous_point_address).unwrap();

    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::NewListenAddr(addr) => {
                println!("Listening on {}", addr);
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                cause: Some(error),
                ..
            } if peer_id == rendezvous_point => {
                println!("Lost connection to rendezvous point {}", error);
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                if peer_id == cli.rendezvous_peer_id {
                    swarm.behaviour_mut().rendezvous.register(
                        Namespace::new("rendezvous".to_string())?,
                        rendezvous_point,
                        None,
                    );
                }
            }
            SwarmEvent::Behaviour(Event::Rendezvous(rendezvous::Event::Registered {
                namespace,
                ttl,
                rendezvous_node,
            })) => {
                println!(
                    "Registered for namespace '{}' at rendezvous point {} for the next {} seconds",
                    namespace, rendezvous_node, ttl
                );
                return Ok(());
            }
            SwarmEvent::Behaviour(Event::Rendezvous(rendezvous::Event::RegisterFailed(error))) => {
                println!("Failed to register {:?}", error);
            }
            other => {
                println!("Unhandled {:?}", other);
            }
        }
    }

    Ok(())
}
