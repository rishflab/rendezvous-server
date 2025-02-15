use anyhow::Result;
use libp2p::core::identity::ed25519::SecretKey;
use libp2p::dns::TokioDnsConfig;
use libp2p::futures::StreamExt;
use libp2p::rendezvous::{Config, Rendezvous};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::tcp::TokioTcpConfig;
use libp2p::{identity, PeerId, Transport};
use rendezvous_server::transport::authenticate_and_multiplex;
use rendezvous_server::{parse_secret_key, Behaviour, Event};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Cli {
    #[structopt(long = "secret-key", help = "32 byte string", parse(try_from_str = parse_secret_key))]
    pub secret_key: SecretKey,
    #[structopt(long = "port")]
    pub port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::from_args();

    let identity = identity::Keypair::Ed25519(cli.secret_key.into());

    let tcp_with_dns = TokioDnsConfig::system(TokioTcpConfig::new().nodelay(true)).unwrap();

    let transport = authenticate_and_multiplex(tcp_with_dns.boxed(), &identity).unwrap();

    let rendezvous = Rendezvous::new(identity.clone(), Config::default());

    let peer_id = PeerId::from(identity.public());

    let mut swarm = SwarmBuilder::new(transport, Behaviour::new(rendezvous), peer_id)
        .executor(Box::new(|f| {
            tokio::spawn(f);
        }))
        .build();

    println!("peer id: {}", swarm.local_peer_id());

    swarm
        .listen_on(format!("/ip4/0.0.0.0/tcp/{}", cli.port).parse().unwrap())
        .unwrap();

    loop {
        let event = swarm.next().await;

        if let Some(event) = event {
            match event {
                SwarmEvent::Behaviour(Event::Ping(_)) => {}
                event => println!("swarm event: {:?}", event),
            }
        }
    }
}
