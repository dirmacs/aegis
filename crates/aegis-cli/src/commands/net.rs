use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;

use anyhow::{Result, bail};
use console::Style;

use aegis_net::ca::CertificateAuthority;
use aegis_net::config::NetworkManifest;
use aegis_net::peer;
use aegis_net::status;
use aegis_net::wg::{self, WgKeypair};

use super::Context;

#[derive(clap::Args)]
pub struct NetArgs {
    #[command(subcommand)]
    pub command: NetCommand,
}

#[derive(clap::Subcommand)]
pub enum NetCommand {
    /// Initialize a new overlay network (creates CA + manifest)
    Init(NetInitArgs),
    /// Show network status and health
    Status(NetStatusArgs),
    /// Add a peer to the network
    AddPeer(AddPeerArgs),
    /// Remove a peer from the network
    RemovePeer(RemovePeerArgs),
    /// List all peers in the network
    ListPeers(NetStatusArgs),
    /// Generate WireGuard configs for all peers
    Generate(NetGenerateArgs),
    /// Sign (or re-sign) a peer's certificate
    Sign(SignArgs),
}

#[derive(clap::Args)]
pub struct NetInitArgs {
    /// Network name
    #[arg(long, default_value = "dirmacs-mesh")]
    pub name: String,
    /// Network CIDR
    #[arg(long, default_value = "10.42.0.0/24")]
    pub cidr: String,
    /// CA certificate duration in days
    #[arg(long, default_value = "365")]
    pub ca_duration: u32,
    /// Config directory
    #[arg(long, default_value = "/etc/aegis-net")]
    pub config_dir: String,
}

#[derive(clap::Args)]
pub struct NetStatusArgs {
    /// Path to aegis-net.toml
    #[arg(long, default_value = "aegis-net.toml")]
    pub manifest: String,
}

#[derive(clap::Args)]
pub struct AddPeerArgs {
    /// Peer name
    pub name: String,
    /// Overlay IP (auto-assigned if omitted)
    #[arg(long)]
    pub ip: Option<IpAddr>,
    /// Groups (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub groups: Vec<String>,
    /// Public endpoint (ip:port)
    #[arg(long)]
    pub endpoint: Option<String>,
    /// Mark as lighthouse
    #[arg(long)]
    pub lighthouse: bool,
    /// Certificate duration in days
    #[arg(long, default_value = "365")]
    pub cert_duration: u32,
    /// Path to aegis-net.toml
    #[arg(long, default_value = "aegis-net.toml")]
    pub manifest: String,
}

#[derive(clap::Args)]
pub struct RemovePeerArgs {
    /// Peer name
    pub name: String,
    /// Path to aegis-net.toml
    #[arg(long, default_value = "aegis-net.toml")]
    pub manifest: String,
}

#[derive(clap::Args)]
pub struct NetGenerateArgs {
    /// Generate for a specific peer (default: all)
    pub peer: Option<String>,
    /// Path to aegis-net.toml
    #[arg(long, default_value = "aegis-net.toml")]
    pub manifest: String,
    /// Output directory for WireGuard configs
    #[arg(long)]
    pub output: Option<String>,
}

#[derive(clap::Args)]
pub struct SignArgs {
    /// Peer name to sign
    pub name: String,
    /// Certificate duration in days
    #[arg(long, default_value = "365")]
    pub duration: u32,
    /// Path to aegis-net.toml
    #[arg(long, default_value = "aegis-net.toml")]
    pub manifest: String,
}

pub async fn run(args: NetArgs, ctx: &Context) -> Result<()> {
    match args.command {
        NetCommand::Init(a) => init(a, ctx).await,
        NetCommand::Status(a) => net_status(a, ctx).await,
        NetCommand::AddPeer(a) => add_peer(a, ctx).await,
        NetCommand::RemovePeer(a) => remove_peer(a, ctx).await,
        NetCommand::ListPeers(a) => list_peers(a, ctx).await,
        NetCommand::Generate(a) => generate(a, ctx).await,
        NetCommand::Sign(a) => sign(a, ctx).await,
    }
}

async fn init(args: NetInitArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();
    let bold = Style::new().bold();
    let config_dir = PathBuf::from(&args.config_dir);

    println!("{}", bold.apply_to("Initializing aegis overlay network..."));

    if ctx.dry_run {
        println!("  [dry-run] Would create CA in {}", config_dir.display());
        println!("  [dry-run] Would create aegis-net.toml");
        return Ok(());
    }

    // Generate CA
    let ca = CertificateAuthority::generate(&args.name, args.ca_duration)?;
    let ca_dir = config_dir.join("ca");
    ca.save(&ca_dir)?;
    println!("  {} CA generated (fingerprint: {})", green.apply_to("✓"), &ca.ca_cert.fingerprint[..16]);

    // Create initial manifest
    let cidr: ipnet::Ipv4Net = args.cidr.parse()
        .map_err(|e| anyhow::anyhow!("invalid CIDR: {e}"))?;

    let manifest = NetworkManifest {
        network: aegis_net::config::NetworkConfig {
            name: args.name.clone(),
            cidr,
            config_dir: config_dir.clone(),
            listen_port: 51820,
            mtu: 1300,
            interface: "aegis0".to_string(),
        },
        lighthouse: None,
        peers: HashMap::new(),
        firewall: aegis_net::config::FirewallConfig::default(),
    };

    let manifest_path = PathBuf::from("aegis-net.toml");
    manifest.save(&manifest_path)?;
    println!("  {} Manifest created: {}", green.apply_to("✓"), manifest_path.display());
    println!("  {} Network: {} ({})", green.apply_to("✓"), args.name, args.cidr);

    println!();
    println!("Next steps:");
    println!("  aegis net add-peer vps --ip 10.42.0.1 --groups servers --lighthouse --endpoint YOUR_IP:51820");
    println!("  aegis net add-peer laptop --groups admin,dev");
    println!("  aegis net generate");

    Ok(())
}

async fn net_status(args: NetStatusArgs, _ctx: &Context) -> Result<()> {
    let bold = Style::new().bold();
    let green = Style::new().green();
    let red = Style::new().red();
    let dim = Style::new().dim();

    let manifest = NetworkManifest::load(&PathBuf::from(&args.manifest))?;
    let config_dir = &manifest.network.config_dir;
    let st = status::check(&manifest, config_dir);

    println!("{}", bold.apply_to(format!("Network: {}", st.network_name)));
    println!();

    // CA status
    let ca_icon = if st.ca_valid { green.apply_to("✓") } else { red.apply_to("✗") };
    let ca_info = st.ca_expires.as_deref().unwrap_or("not found");
    println!("  {ca_icon} CA: expires {ca_info}");

    // WireGuard
    let wg_icon = if st.wg_available { green.apply_to("✓") } else { red.apply_to("✗") };
    println!("  {wg_icon} WireGuard: {}", if st.wg_available { "available" } else { "not found" });

    let iface_icon = if st.wg_interface_up { green.apply_to("✓") } else { dim.apply_to("○") };
    println!("  {iface_icon} Interface: {}", if st.wg_interface_up { "up" } else { "down" });

    // Peers
    println!("  {} Peers: {}/{} have certificates", green.apply_to("✓"), st.peers_with_certs, st.total_peers);

    if !st.expired_certs.is_empty() {
        println!("  {} Expired certs: {}", red.apply_to("!"), st.expired_certs.join(", "));
    }

    Ok(())
}

async fn add_peer(args: AddPeerArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();
    let manifest_path = PathBuf::from(&args.manifest);
    let mut manifest = NetworkManifest::load(&manifest_path)?;
    let config_dir = manifest.network.config_dir.clone();

    let ip = match args.ip {
        Some(ip) => ip,
        None => peer::next_available_ip(&manifest)
            .ok_or_else(|| anyhow::anyhow!("no available IPs in network CIDR"))?,
    };

    if ctx.dry_run {
        println!("  [dry-run] Would add peer '{}' ({}) groups={:?}", args.name, ip, args.groups);
        return Ok(());
    }

    let ca = CertificateAuthority::load(&config_dir.join("ca"))?;

    // Generate WireGuard keypair too
    let wg_kp = WgKeypair::generate();
    let peer_dir = config_dir.join("peers").join(&args.name);
    wg_kp.save(&peer_dir, &args.name)?;

    let cert = peer::add_peer(
        &mut manifest,
        &ca,
        &args.name,
        ip,
        args.groups.clone(),
        args.endpoint.clone(),
        args.lighthouse,
        args.cert_duration,
        &config_dir,
    )?;

    manifest.save(&manifest_path)?;

    println!("  {} Added peer '{}' ({})", green.apply_to("✓"), args.name, ip);
    println!("    Groups: {:?}", args.groups);
    if args.lighthouse {
        println!("    Role: lighthouse");
    }
    println!("    Cert expires: {}", cert.expires_at.format("%Y-%m-%d"));
    println!("    WG pubkey: {}", wg_kp.public_key_base64());

    Ok(())
}

async fn remove_peer(args: RemovePeerArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();
    let manifest_path = PathBuf::from(&args.manifest);
    let mut manifest = NetworkManifest::load(&manifest_path)?;

    if ctx.dry_run {
        println!("  [dry-run] Would remove peer '{}'", args.name);
        return Ok(());
    }

    let removed = peer::remove_peer(&mut manifest, &args.name)?;
    manifest.save(&manifest_path)?;

    println!("  {} Removed peer '{}' ({})", green.apply_to("✓"), args.name, removed.ip);
    Ok(())
}

async fn list_peers(args: NetStatusArgs, _ctx: &Context) -> Result<()> {
    let bold = Style::new().bold();
    let dim = Style::new().dim();

    let manifest = NetworkManifest::load(&PathBuf::from(&args.manifest))?;
    let peers = peer::list_peers(&manifest);

    if peers.is_empty() {
        println!("No peers configured. Use 'aegis net add-peer' to add one.");
        return Ok(());
    }

    println!("{}", bold.apply_to(format!("Peers in {} ({}):", manifest.network.name, manifest.network.cidr)));
    println!();

    for p in &peers {
        let role = if p.lighthouse { " [lighthouse]" } else { "" };
        let ep = p.endpoint.as_deref().unwrap_or("no endpoint");
        println!("  {:<16} {:<16} {:?}{}", p.name, p.ip, p.groups, role);
        println!("  {}", dim.apply_to(format!("                 {}", ep)));
    }

    Ok(())
}

async fn generate(args: NetGenerateArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();
    let manifest = NetworkManifest::load(&PathBuf::from(&args.manifest))?;
    let config_dir = &manifest.network.config_dir;

    // Load all WireGuard public keys
    let mut pub_keys: HashMap<String, String> = HashMap::new();
    for name in manifest.peers.keys() {
        let pub_path = config_dir.join("peers").join(name).join(format!("{}.wg.pub", name));
        if let Ok(key) = WgKeypair::load_public(&pub_path) {
            pub_keys.insert(name.clone(), base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                key.as_bytes(),
            ));
        } else {
            tracing::warn!("no WG public key for peer '{}' at {}", name, pub_path.display());
        }
    }

    let output_dir = args.output
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir.join("wg-configs"));

    let peers_to_generate: Vec<&String> = if let Some(ref name) = args.peer {
        if !manifest.peers.contains_key(name) {
            bail!("peer '{}' not found", name);
        }
        vec![name]
    } else {
        manifest.peers.keys().collect()
    };

    for name in peers_to_generate {
        let priv_path = config_dir.join("peers").join(name).join(format!("{}.wg.key", name));
        let priv_key = match std::fs::read_to_string(&priv_path) {
            Ok(k) => k.trim().to_string(),
            Err(_) => {
                tracing::warn!("no private key for '{}', skipping", name);
                continue;
            }
        };

        let config = wg::generate_config(&manifest, name, &priv_key, &pub_keys)?;

        if ctx.dry_run {
            println!("  [dry-run] Would write config for '{}'", name);
            continue;
        }

        let path = wg::write_config(&config, &output_dir, &format!("aegis0-{}", name))?;
        println!("  {} Generated: {}", green.apply_to("✓"), path.display());
    }

    Ok(())
}

async fn sign(args: SignArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();
    let manifest = NetworkManifest::load(&PathBuf::from(&args.manifest))?;
    let config_dir = &manifest.network.config_dir;

    let peer = manifest.peer(&args.name)
        .ok_or_else(|| anyhow::anyhow!("peer '{}' not found", args.name))?;

    if ctx.dry_run {
        println!("  [dry-run] Would re-sign cert for '{}'", args.name);
        return Ok(());
    }

    let ca = CertificateAuthority::load(&config_dir.join("ca"))?;

    // Load existing node public key
    let peer_dir = config_dir.join("peers").join(&args.name);
    let cert_path = peer_dir.join(format!("{}.cert", args.name));
    let existing = aegis_net::ca::NodeCertificate::load(&cert_path)?;

    let cert = ca.sign_node(
        &args.name,
        &peer.ip.to_string(),
        &peer.groups,
        &existing.public_key,
        args.duration,
    )?;
    cert.save(&peer_dir)?;

    println!("  {} Re-signed cert for '{}' (expires {})", green.apply_to("✓"), args.name, cert.expires_at.format("%Y-%m-%d"));

    Ok(())
}
