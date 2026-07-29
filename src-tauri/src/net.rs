//! Getting a hosted world reachable from outside the house.
//!
//! Binding to `0.0.0.0:5520` only makes the world visible on the local network.
//! For friends elsewhere the router has to forward that UDP port inward, which
//! is what UPnP automates — and when UPnP can't work, saying so plainly beats
//! leaving someone staring at a world nobody can join.

use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use igd_next::{search_gateway, PortMappingProtocol, SearchOptions};

/// Renew well before expiry; routers drop mappings at the lease boundary.
const LEASE_SECONDS: u32 = 3600;

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reachability {
    /// LAN address friends on the same network can use.
    pub local_address: Option<String>,
    /// Internet address, once a router has told us what it is.
    pub external_address: Option<String>,
    /// True when a port mapping is actually in place.
    pub mapped: bool,
    /// Plain-language state for the UI.
    pub detail: String,
    /// Set when the router's own WAN address is itself private, i.e. the ISP
    /// is doing carrier-grade NAT and no amount of UPnP will help.
    pub behind_cgnat: bool,
}

/// Our address on the LAN.
///
/// Connecting a UDP socket sends nothing; it just asks the routing table which
/// interface would be used to reach the internet, which is the address we want.
pub fn local_ip() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("1.1.1.1:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
        _ => None,
    }
}

/// Addresses that are not routable on the public internet. A router reporting
/// one of these as its *external* address means we're behind another layer of
/// NAT we cannot open.
fn is_unroutable(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        // 100.64.0.0/10 — the carrier-grade NAT range (RFC 6598).
        || (a == 100 && (64..128).contains(&b))
}

/// Ask the router to forward `port` inward, and report what happened.
///
/// Every failure here is non-fatal: the world still runs and is still playable
/// on the LAN, so we downgrade to an explanation rather than an error.
pub fn open_port(port: u16) -> Reachability {
    let mut out = Reachability {
        local_address: local_ip().map(|ip| format!("{ip}:{port}")),
        ..Default::default()
    };

    let Some(local) = local_ip() else {
        out.detail = "Couldn't determine this machine's network address.".into();
        return out;
    };

    let opts = SearchOptions {
        timeout: Some(Duration::from_secs(5)),
        ..Default::default()
    };

    let gateway = match search_gateway(opts) {
        Ok(g) => g,
        Err(e) => {
            out.detail = format!(
                "No UPnP router found ({e}). Friends on your network can join at \
                 {}, but anyone outside will need you to forward UDP {port} manually.",
                out.local_address.clone().unwrap_or_default()
            );
            return out;
        }
    };

    // Ask the router for its WAN address first: if that is itself private, the
    // ISP is doing carrier-grade NAT and a port mapping cannot help.
    match gateway.get_external_ip() {
        Ok(IpAddr::V4(wan)) => {
            out.external_address = Some(format!("{wan}:{port}"));
            if is_unroutable(wan) {
                out.behind_cgnat = true;
                out.detail = format!(
                    "Your router's internet address ({wan}) is itself private, which means \
                     your ISP uses carrier-grade NAT. Port forwarding can't get through it. \
                     Friends on your own network can still join at {}.",
                    out.local_address.clone().unwrap_or_default()
                );
                return out;
            }
        }
        Ok(IpAddr::V6(v6)) => out.external_address = Some(format!("[{v6}]:{port}")),
        Err(e) => out.detail = format!("Router wouldn't report its internet address ({e}). "),
    }

    let target = SocketAddr::new(IpAddr::V4(local), port);
    match gateway.add_port(
        PortMappingProtocol::UDP,
        port,
        target,
        LEASE_SECONDS,
        "HyPortal - Hytale world",
    ) {
        Ok(()) => {
            out.mapped = true;
            out.detail = match &out.external_address {
                Some(addr) => format!("Port open. Friends can join at {addr}."),
                None => format!("Port UDP {port} opened on your router."),
            };
        }
        Err(e) => {
            out.detail = format!(
                "Router refused to open the port ({e}). It may have UPnP disabled. \
                 Friends on your network can still join at {}.",
                out.local_address.clone().unwrap_or_default()
            );
        }
    }

    out
}

/// Remove a mapping we made. Best-effort — routers expire the lease anyway.
pub fn close_port(port: u16) {
    let opts = SearchOptions {
        timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    };
    if let Ok(gateway) = search_gateway(opts) {
        let _ = gateway.remove_port(PortMappingProtocol::UDP, port);
    }
}
