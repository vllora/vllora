use crate::config::Config;
use crate::CliError;

#[derive(Debug)]
pub enum Service {
    Backend,
    UI,
    Otel,
}

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Service::Backend => write!(f, "Backend"),
            Service::UI => write!(f, "UI"),
            Service::Otel => write!(f, "OTEL"),
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
    ];

    let mut excluded_ports = services
        .iter()
        .map(|service| service.initial_port)
        .collect::<Vec<u16>>();
    for service in &mut services {
        if !is_port_available(service.host.clone(), service.initial_port).await {
            let new_port = find_next_available_port(
                service.host.clone(),
                service.initial_port,
                &excluded_ports,
            )
            .await;
            if let Some(new_port) = new_port {
                service.suggested_port = Some(new_port);
                excluded_ports.push(new_port);
            } else {
                return Err(CliError::IoError(std::io::Error::other(format!(
                    "No available port found for service {}",
                    service.service
                ))));
            }
        }
    }

    Ok(services)
}

async fn is_port_available(host: String, port: u16) -> bool {
    (tokio::net::TcpListener::bind(format!("{host}:{port}")).await).is_ok()
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
