use fast_core::base_types::*;
use failure::Error;
use log::info;
use rand::rngs::OsRng;
use rand::TryRngCore;
use serde::{ Deserialize, Serialize };
use std::fs::{ self, File };
use std::io::BufWriter;
use std::path::Path;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct BridgeConfigGenOpt {
    /// Number of authorities
    #[structopt(long, default_value = "4")]
    num_authorities: usize,

    /// Number of shards per authority
    #[structopt(long, default_value = "16")]
    num_shards: u32,

    /// Host for authorities
    #[structopt(long, default_value = "127.0.0.1")]
    host: String,

    /// Base port for authorities
    #[structopt(long, default_value = "8000")]
    base_port: u16,

    /// Port step between authorities
    #[structopt(long, default_value = "100")]
    port_step: u16,

    /// Output directory
    #[structopt(long, default_value = "./bridge_config")]
    output_dir: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthorityConfig {
    name: String,
    secret_key: String,
    committee: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CommitteeConfig {
    authorities: Vec<AuthorityEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthorityEntry {
    name: String,
    host: String,
    port: u16,
    weight: u64,
    num_shards: u32,
}

fn generate_keypair() -> (Pubkey, [u8; 32]) {
    let mut rng = OsRng;
    let mut secret = [0u8; 32];
    rng.try_fill_bytes(&mut secret).expect("Failed to fill secret key with random bytes");
    let keypair = KeyPair::from(secret);
    let public = keypair.public();

    (public, secret)
}

/// Encode a public key as a hex string
fn encode_public_key(key: &Pubkey) -> String {
    hex::encode(key.0)
}

/// Encode a secret key as a hex string
fn encode_secret_key(key: &[u8; 32]) -> String {
    hex::encode(key)
}

/// Generate bridge configuration
pub async fn generate_bridge_config(opt: BridgeConfigGenOpt) -> Result<(), Error> {
    // Create output directory
    fs::create_dir_all(&opt.output_dir)?;

    // Generate authorities
    let mut authority_entries = Vec::new();

    for i in 0..opt.num_authorities {
        // Generate keypair
        let (public_key, secret_key) = generate_keypair();

        // Create authority entry
        let authority_entry = AuthorityEntry {
            name: encode_public_key(&public_key),
            host: opt.host.clone(),
            port: opt.base_port + (i as u16) * opt.port_step,
            weight: 1,
            num_shards: opt.num_shards,
        };

        authority_entries.push(authority_entry);

        // Create authority config
        let authority_config = AuthorityConfig {
            name: encode_public_key(&public_key),
            secret_key: encode_secret_key(&secret_key),
            committee: Path::new(&opt.output_dir)
                .join("committee.json")
                .to_str()
                .unwrap()
                .to_string(),
        };

        // Save authority config
        let config_path = Path::new(&opt.output_dir).join(format!("authority_{}.json", i));
        let file = File::create(config_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &authority_config)?;
    }

    // Create committee config
    let committee_config = CommitteeConfig {
        authorities: authority_entries,
    };

    // Save committee config
    let committee_path = Path::new(&opt.output_dir).join("committee.json");
    let file = File::create(committee_path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &committee_config)?;

    info!("Bridge configuration generated in: {}", opt.output_dir);

    Ok(())
}
