//! Which network destinations the proxy may reach (SPEC-0003 FR-201).
//!
//! The sandbox profile denies the command loopback, the LAN and link-local
//! addresses directly. This module decides the same question for the relay,
//! so that what the profile denies is not simply reached on the command's
//! behalf.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Why a destination was refused, worded so the user knows what to change.
pub const RESTRICTED_DESTINATION: &str = "loopback, private and link-local destinations are not \
     relayed; set allow_private_egress (SANDME_ALLOW_PRIVATE_EGRESS=1) to permit them";

/// Where a request may go, once its destination has been resolved.
pub enum Destination {
    /// Addresses the proxy may connect to.
    Permitted(Vec<SocketAddr>),
    /// At least one address is one FR-201 refuses.
    Restricted,
    /// The destination names nothing the resolver knows.
    Unresolvable,
}

/// Resolve a destination and decide whether the proxy may reach it (FR-201).
///
/// The name is resolved here rather than only checked when it is already an
/// IP address: `localtest.me` and friends resolve to `127.0.0.1`, and a check
/// that misses them is decoration. A name that yields even one restricted
/// address is refused whole — allowing the rest would leave the client to
/// pick, and the client is what this rule constrains (FR-206).
pub async fn resolve(host: &str, port: u16, allow_private_egress: bool) -> Destination {
    let Ok(addresses) = tokio::net::lookup_host((host, port)).await else {
        return Destination::Unresolvable;
    };
    let addresses: Vec<SocketAddr> = addresses.collect();

    if addresses.is_empty() {
        Destination::Unresolvable
    } else if !allow_private_egress && addresses.iter().any(|address| is_restricted(address.ip())) {
        Destination::Restricted
    } else {
        Destination::Permitted(addresses)
    }
}

/// Whether `ip` is one of the addresses the sandbox denies the child directly.
fn is_restricted(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(address) => is_restricted_v4(address),
        IpAddr::V6(address) => is_restricted_v6(address),
    }
}

/// Carrier-grade NAT, `100.64.0.0/10` (RFC 6598). Tailscale assigns tailnet
/// addresses from this range, so relaying to it reaches peers the sandbox
/// profile denies the command directly — the pivot issue #15 was filed about
/// (issue #32).
const CGNAT_NETWORK: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 0);
const CGNAT_PREFIX: u32 = 10;

/// Benchmarking, `198.18.0.0/15` (RFC 2544). Reserved for device testing and
/// not a destination a sandboxed command has any reason to reach (issue #32).
const BENCHMARK_NETWORK: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 0);
const BENCHMARK_PREFIX: u32 = 15;

/// Loopback, RFC1918, link-local — including `169.254.169.254`, the cloud
/// metadata endpoint — the unspecified address, which macOS routes to
/// loopback, and the carrier-grade NAT and benchmarking ranges `std` cannot
/// classify (issue #32).
fn is_restricted_v4(address: Ipv4Addr) -> bool {
    address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_unspecified()
        || in_cidr_v4(address, CGNAT_NETWORK, CGNAT_PREFIX)
        || in_cidr_v4(address, BENCHMARK_NETWORK, BENCHMARK_PREFIX)
}

/// Whether `address` falls inside the `network`/`prefix` CIDR block.
///
/// `std` has no CIDR type and no predicate for these ranges, so membership is
/// a masked comparison: the top `prefix` bits of the address must equal the
/// network's.
fn in_cidr_v4(address: Ipv4Addr, network: Ipv4Addr, prefix: u32) -> bool {
    let mask = u32::MAX.checked_shl(u32::BITS - prefix).unwrap_or(0);
    u32::from(address) & mask == u32::from(network) & mask
}

/// `::1`, `::`, `fe80::/10` and `fc00::/7`, plus any IPv4 address carried
/// inside an IPv6 one.
///
/// `std`'s predicates for the last two ranges are still unstable, so the
/// prefixes are matched on the address's first segment. `::ffff:127.0.0.1`
/// is the obvious way around a check that looks only at IPv6 properties, so
/// an embedded IPv4 address is classified as that address.
fn is_restricted_v6(address: Ipv6Addr) -> bool {
    const LINK_LOCAL: u16 = 0xfe80;
    const LINK_LOCAL_MASK: u16 = 0xffc0;
    const UNIQUE_LOCAL: u16 = 0xfc00;
    const UNIQUE_LOCAL_MASK: u16 = 0xfe00;

    let leading = address.segments()[0];
    address.is_loopback()
        || address.is_unspecified()
        || leading & LINK_LOCAL_MASK == LINK_LOCAL
        || leading & UNIQUE_LOCAL_MASK == UNIQUE_LOCAL
        || address.to_ipv4().is_some_and(is_restricted_v4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricts_every_range_the_sandbox_denies_directly() {
        // Given one address from each range FR-201 names
        for address in [
            "127.0.0.1",
            "127.1.2.3",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.88.218",
            "169.254.1.1",
            "169.254.169.254", // the cloud metadata endpoint
            "0.0.0.0",
            "::1",
            "::",
            "fe80::1",
            "febf::1",
            "fc00::1",
            "fd12:3456::1",
            "100.64.0.1",       // carrier-grade NAT, 100.64.0.0/10 (Tailscale)
            "100.127.255.255",  // the top of the CGNAT range
            "198.18.0.1",       // benchmarking, 198.18.0.0/15
            "198.19.255.255",   // the top of the benchmarking range
            "::ffff:127.0.0.1", // IPv4-mapped
            "::ffff:10.1.2.3",
            "::ffff:100.64.0.1", // an IPv4-mapped CGNAT address
            "::127.0.0.1",       // IPv4-compatible
        ] {
            // Then the proxy will not relay to it
            let ip: IpAddr = address.parse().unwrap();
            assert!(is_restricted(ip), "{address} should be restricted");
        }
    }

    #[test]
    fn relays_to_ordinary_public_addresses() {
        // Given addresses just outside each restricted range, and two real
        // public ones
        for address in [
            "93.184.216.34",
            "8.8.8.8",
            "172.32.0.1",     // one past 172.16/12
            "172.15.0.1",     // one before it
            "169.253.1.1",    // one before 169.254/16
            "100.63.255.255", // one before 100.64/10 (CGNAT)
            "100.128.0.1",    // one past it
            "198.17.255.255", // one before 198.18/15 (benchmarking)
            "198.20.0.1",     // one past it
            "11.0.0.1",
            "2606:2800:220:1:248:1893:25c8:1946",
            "fe00::1", // one before fc00::/7
            "fec0::1", // one past fe80::/10
            "2001::1",
        ] {
            // Then nothing stops the proxy relaying to it
            let ip: IpAddr = address.parse().unwrap();
            assert!(!is_restricted(ip), "{address} should not be restricted");
        }
    }

    #[tokio::test]
    async fn restricts_a_hostname_that_resolves_into_a_restricted_range() {
        // Given a name — not a literal address — that resolves to loopback
        // When the destination is resolved with the default configuration
        // Then it is refused: a check that only read literal IPs would let
        // every `localtest.me`-style name straight through (FR-206)
        assert!(matches!(
            resolve("localhost", 80, false).await,
            Destination::Restricted
        ));

        // And with the opt-out it resolves to addresses the proxy may use
        assert!(matches!(
            resolve("localhost", 80, true).await,
            Destination::Permitted(addresses) if !addresses.is_empty()
        ));
    }

    #[tokio::test]
    async fn gates_cgnat_and_benchmarking_on_private_egress() {
        // Given a carrier-grade NAT and a benchmarking address — the ranges
        // issue #32 found relayed on defaults
        for address in ["100.64.0.1", "198.18.0.1"] {
            // When the destination is resolved with the default configuration
            // Then it is refused, exactly like loopback and RFC1918 (FR-1104)
            assert!(
                matches!(resolve(address, 80, false).await, Destination::Restricted),
                "{address} should be restricted by default"
            );

            // And the opt-out widens it uniformly, as it does the other ranges
            assert!(
                matches!(
                    resolve(address, 80, true).await,
                    Destination::Permitted(addresses) if !addresses.is_empty()
                ),
                "{address} should be permitted under allow_private_egress"
            );
        }
    }
}
