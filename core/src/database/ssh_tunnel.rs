use openssh::{ForwardType, KnownHosts, Session, SessionBuilder, Socket};
use std::fs::File;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use thiserror::Error;

use crate::types::db_connection::SshSettings;

#[derive(Debug, Error)]
pub enum SshTunnelError {
    #[error("io error: {0}")]
    IoError(std::io::Error),
    #[error("ssh error: {0}")]
    SshError(openssh::Error),
    // Add other error types as needed
}

impl From<std::io::Error> for SshTunnelError {
    fn from(error: std::io::Error) -> Self {
        SshTunnelError::IoError(error)
    }
}
impl From<openssh::Error> for SshTunnelError {
    fn from(error: openssh::Error) -> Self {
        SshTunnelError::SshError(error)
    }
}
// #[warn(dead_code)]
// async fn get_random_localhost_port() -> Result<u16, io::Error> {
//     // The 0 port indicates to the OS to assign a random port
//     let listener = TcpListener::bind("localhost:0").await.map_err(|e| {
//         io::Error::new(
//             io::ErrorKind::Other,
//             format!("Failed to bind to a random port due to {e}"),
//         )
//     })?;
//     let addr = listener.local_addr()?;
//     Ok(addr.port())
// }

async fn generate_temp_keyfile(private_key: &str) -> Result<PathBuf, SshTunnelError> {
    // Create a temporary file path
    let mut temp_keyfile_path = std::env::temp_dir();
    temp_keyfile_path.push(format!("ssh_tunnel_key-{}.pem", uuid::Uuid::new_v4()));
    // Create and write the private key to the temporary file
    let mut temp_keyfile = File::create(&temp_keyfile_path)?;

    temp_keyfile.write_all(private_key.as_bytes())?;

    // Set the file permissions to only owner read and write
    let mut permissions = temp_keyfile.metadata()?.permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o600);
    temp_keyfile.set_permissions(permissions)?;

    // Return the path of the temporary file
    Ok(temp_keyfile_path)
}
pub async fn cleanup_tunnel(
    session: Session,
    path: &PathBuf,
    port: u16,
    server_port: u16,
) -> Result<(), SshTunnelError> {
    // Close the port forwarding and SSH session
    let local = Socket::from(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    let remote = Socket::from(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        server_port,
    ));
    session
        .close_port_forward(ForwardType::Local, local, remote)
        .await?;
    session.close().await?;
    // clean up the temp file
    std::fs::remove_file(path)?;
    Ok(())
}
pub async fn create_tunnel(
    setting: SshSettings,
    server_port: u16,
) -> Result<(Session, PathBuf, u16), SshTunnelError> {
    // create a temp file
    let temp_keyfile = generate_temp_keyfile(&setting.private_key).await?;
    let moved_keyfile = &temp_keyfile; // Borrow the value

    //let key = openssh::PrivateKey::from_keystr(&key_str, None).unwrap();
    // Establish the SSH session
    let mut builder = SessionBuilder::default();
    builder
        .user(setting.username)
        .keyfile(moved_keyfile)
        .known_hosts_check(KnownHosts::Accept);
    // .keyfile(moved_keyfile)
    // .known_hosts_check(KnownHosts::Accept);
    if !setting.jump_servers.is_empty() {
        builder.jump_hosts(setting.jump_servers);
    }

    let session = builder.connect(setting.host).await?;
    // Ensure the session is active
    session.check().await?;

    // Set up local port forwarding
    let port = server_port; //get_random_localhost_port().await.unwrap(); //8443;
    let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), server_port);

    let local = Socket::from(local_addr);
    let remote = Socket::from(remote_addr);

    session
        .request_port_forward(ForwardType::Local, local.clone(), remote.clone())
        .await
        .map_err(|e| {
            eprintln!("Error establishing port forwarding: {}", e);
            e
        })?;

    println!("Port forwarding established on port {}", port);

    Ok((session, moved_keyfile.to_owned(), port))
}
