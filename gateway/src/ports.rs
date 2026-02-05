use crate::config::Config;
use crate::CliError;
use tracing::debug;

#[derive(Debug)]
pub enum Service {
    Backend,
    UI,
    Otel,
    Distri,
}

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Service::Backend => write!(f, "Backend"),
            Service::UI => write!(f, "UI"),
            Service::Otel => write!(f, "OTEL"),
            Service::Distri => write!(f, "Distri"),
        }
    }
}

pub struct ServicePort {
    pub service: Service,
    pub initial_port: u16,
    pub host: String,
    pub suggested_port: Option<u16>,
}

pub async fn resolve_ports(config: &Config) -> Result<Vec<ServicePort>, CliError> {
    let mut services = vec![
        ServicePort {
            service: Service::Backend,
            initial_port: config.http.port,
            host: config.http.host.clone(),
            suggested_port: None,
        },
        ServicePort {
            service: Service::UI,
            initial_port: config.ui.port,
            host: config.http.host.clone(),
            suggested_port: None,
        },
        ServicePort {
            service: Service::Otel,
            initial_port: config.otel.port,
            host: config.otel.host.clone(),
            suggested_port: None,
        },
        ServicePort {
            service: Service::Distri,
            initial_port: config.distri.port,
            host: config.http.host.clone(),
            suggested_port: None,
        },
    ];

    let mut excluded_ports = services
        .iter()
        .map(|service| service.initial_port)
        .collect::<Vec<u16>>();

    let ipv6_supported = is_ipv6_supported().await;
    debug!("IPv6 support detected: {}", ipv6_supported);

    for service in &mut services {
        let port_check = is_port_available_detailed(service.host.clone(), service.initial_port, ipv6_supported).await;
        if !port_check.is_available {
            debug!(
                "{} service: port {} is in use - {}",
                service.service, service.initial_port, port_check.reason
            );
            let new_port = find_next_available_port(
                service.host.clone(),
                service.initial_port,
                &excluded_ports,
            )
            .await;
            if let Some(new_port) = new_port {
                debug!(
                    "{} service: next available port found -> {}",
                    service.service, new_port
                );
                service.suggested_port = Some(new_port);
                excluded_ports.push(new_port);
            } else {
                return Err(CliError::IoError(std::io::Error::other(format!(
                    "No available port found for service {}",
                    service.service
                ))));
            }
        } else {
            debug!(
                "{} service: port {} is available",
                service.service, service.initial_port
            );
        }
    }

    Ok(services)
}

struct PortCheckResult {
    is_available: bool,
    reason: String,
}

/// Check if IPv6 is supported on this system
async fn is_ipv6_supported() -> bool {
    tokio::net::TcpListener::bind("[::1]:0").await.is_ok()
}

async fn is_port_available(host: String, port: u16) -> bool {
    let ipv6_supported = is_ipv6_supported().await;
    is_port_available_detailed(host, port, ipv6_supported).await.is_available
}

async fn is_port_available_detailed(host: String, port: u16, ipv6_supported: bool) -> PortCheckResult {
    match tokio::net::TcpListener::bind(format!("{host}:{port}")).await {
        Ok(_) => PortCheckResult {
            is_available: true,
            reason: "available".to_string(),
        },
        Err(e) if !ipv6_supported && (e.raw_os_error() == Some(97) || e.kind() == std::io::ErrorKind::Unsupported) => {
            // IPv6 not supported (error 97), try IPv4 fallback
            let ipv4_host = if host == "[::]" {
                "0.0.0.0"
            } else {
                &host
            };

            debug!("IPv6 not supported, trying IPv4 fallback for {}:{}", ipv4_host, port);

            match tokio::net::TcpListener::bind(format!("{ipv4_host}:{port}")).await {
                Ok(_) => PortCheckResult {
                    is_available: true,
                    reason: "available (IPv4 fallback)".to_string(),
                },
                Err(e2) => PortCheckResult {
                    is_available: false,
                    reason: format!("IPv6 failed: {}, IPv4 failed: {} (kind: {:?})", e, e2, e2.kind()),
                },
            }
        }
        Err(e) => PortCheckResult {
            is_available: false,
            reason: format!("{} (kind: {:?})", e, e.kind()),
        },
    }
}

async fn find_next_available_port(
    host: String,
    start_port: u16,
    exclude_ports: &[u16],
) -> Option<u16> {
    let mut port = start_port;
    // Try up to 1000 ports
    for _ in 0..1000 {
        if !exclude_ports.contains(&port) && is_port_available(host.clone(), port).await {
            return Some(port);
        }
        port += 1;
        // Don't go beyond u16::MAX
        if port == 0 {
            break;
        }
    }
    None
}
