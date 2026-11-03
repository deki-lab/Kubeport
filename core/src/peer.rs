//! Who opened a local connection: a Forward only serves the user running Kubeport.
//!
//! The local port is on the loopback, but every account on the Mac can reach the
//! loopback. TCP carries no credentials, so the connecting socket is looked up in the
//! kernel's tables: its owner must be the user running Kubeport. A socket that cannot
//! be found (another account's process, which an unprivileged process cannot inspect,
//! or a client already gone) is refused.

use std::net::SocketAddr;

/// True when the client socket connected from `peer` to `local` belongs to this user.
/// Blocking: reads the kernel's process or socket tables.
pub(crate) fn same_user(local: SocketAddr, peer: SocketAddr) -> bool {
    imp::owner(local, peer) == Some(current_uid())
}

fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

#[cfg(target_os = "macos")]
mod imp {
    //! The client socket is held by a process of this user: libproc lists the sockets
    //! of every process this user may inspect, which excludes other accounts.

    use std::ffi::c_void;
    use std::net::{IpAddr, SocketAddr};
    use std::sync::Mutex;

    // `struct socket_fdinfo` (sys/proc_info.h) is not in the libc crate: read its
    // fields at their offsets. The layout is ABI, shared with lsof and netstat.
    const SOCKET_FDINFO_SIZE: usize = 792;
    const PROC_PIDFDSOCKETINFO: i32 = 3;
    const SOI_KIND: usize = 256;
    const SOCKINFO_TCP: i32 = 2;
    const INSI_FPORT: usize = 264;
    const INSI_LPORT: usize = 268;
    const INSI_VFLAG: usize = 288;
    const INSI_FADDR: usize = 296;
    const INSI_LADDR: usize = 312;
    const INI_IPV4: u8 = 1;
    const INI_IPV6: u8 = 2;

    /// Processes that opened recent connections: a client usually opens several.
    static RECENT: Mutex<Vec<i32>> = Mutex::new(Vec::new());
    const RECENT_MAX: usize = 8;

    pub(super) fn owner(local: SocketAddr, peer: SocketAddr) -> Option<u32> {
        let recent = RECENT.lock().map(|r| r.clone()).unwrap_or_default();
        let pid = recent
            .iter()
            .copied()
            .find(|&pid| holds(pid, local, peer))
            .or_else(|| all_pids().into_iter().find(|&pid| holds(pid, local, peer)))?;
        if let Ok(mut r) = RECENT.lock() {
            r.retain(|&p| p != pid);
            r.insert(0, pid);
            r.truncate(RECENT_MAX);
        }
        uid_of(pid)
    }

    fn all_pids() -> Vec<i32> {
        // SAFETY: a null buffer asks for the number of processes.
        let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        let Ok(count) = usize::try_from(count) else {
            return Vec::new();
        };
        // Room for processes started between the two calls.
        let mut pids = vec![0i32; count + 64];
        let bytes = i32::try_from(pids.len() * size_of::<i32>()).unwrap_or(i32::MAX);
        // SAFETY: the buffer holds `bytes` bytes.
        let n = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast::<c_void>(), bytes) };
        pids.truncate(usize::try_from(n).unwrap_or(0));
        pids
    }

    /// Whether `pid` holds the client end of the connection `peer` -> `local`.
    fn holds(pid: i32, local: SocketAddr, peer: SocketAddr) -> bool {
        // SAFETY: a null buffer asks for the size of the descriptor list.
        let size =
            unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
        let Ok(size) = usize::try_from(size) else {
            return false;
        };
        let mut fds = vec![
            libc::proc_fdinfo {
                proc_fd: 0,
                proc_fdtype: 0
            };
            size / size_of::<libc::proc_fdinfo>() + 16
        ];
        let bytes = i32::try_from(fds.len() * size_of::<libc::proc_fdinfo>()).unwrap_or(i32::MAX);
        // SAFETY: the buffer holds `bytes` bytes.
        let filled = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDLISTFDS,
                0,
                fds.as_mut_ptr().cast::<c_void>(),
                bytes,
            )
        };
        fds.truncate(usize::try_from(filled).unwrap_or(0) / size_of::<libc::proc_fdinfo>());
        fds.iter()
            .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
            .any(|fd| socket_matches(pid, fd.proc_fd, local, peer))
    }

    fn socket_matches(pid: i32, fd: i32, local: SocketAddr, peer: SocketAddr) -> bool {
        let mut info = [0u8; SOCKET_FDINFO_SIZE];
        // SAFETY: the buffer holds SOCKET_FDINFO_SIZE bytes.
        let filled = unsafe {
            libc::proc_pidfdinfo(
                pid,
                fd,
                PROC_PIDFDSOCKETINFO,
                info.as_mut_ptr().cast::<c_void>(),
                SOCKET_FDINFO_SIZE as i32,
            )
        };
        if usize::try_from(filled) != Ok(SOCKET_FDINFO_SIZE)
            || int_at(&info, SOI_KIND) != SOCKINFO_TCP
        {
            return false;
        }
        // The client's own end: from `peer`, to `local`.
        port_at(&info, INSI_LPORT) == peer.port()
            && port_at(&info, INSI_FPORT) == local.port()
            && addr_matches(&info, INSI_LADDR, peer.ip())
            && addr_matches(&info, INSI_FADDR, local.ip())
    }

    fn int_at(info: &[u8], at: usize) -> i32 {
        i32::from_ne_bytes(info[at..at + 4].try_into().expect("4 bytes"))
    }

    /// A port, stored in network order in the low 16 bits of an int.
    fn port_at(info: &[u8], at: usize) -> u16 {
        u16::from_be(int_at(info, at) as u16)
    }

    fn addr_matches(info: &[u8], at: usize, ip: IpAddr) -> bool {
        let vflag = info[INSI_VFLAG];
        match ip {
            // `struct in4in6_addr`: 12 bytes of padding, then the IPv4 address.
            IpAddr::V4(ip) => vflag & INI_IPV4 != 0 && info[at + 12..at + 16] == ip.octets(),
            IpAddr::V6(ip) => vflag & INI_IPV6 != 0 && info[at..at + 16] == ip.octets(),
        }
    }

    fn uid_of(pid: i32) -> Option<u32> {
        // SAFETY: proc_bsdinfo is plain data; zero is a valid value.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = size_of::<libc::proc_bsdinfo>() as i32;
        // SAFETY: the buffer is one proc_bsdinfo.
        let filled = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&raw mut info).cast::<c_void>(),
                size,
            )
        };
        (filled == size).then_some(info.pbi_uid)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    //! The kernel lists every TCP socket with its owner in /proc/net/tcp and tcp6.

    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    pub(super) fn owner(local: SocketAddr, peer: SocketAddr) -> Option<u32> {
        let table = match peer {
            SocketAddr::V4(_) => "/proc/net/tcp",
            SocketAddr::V6(_) => "/proc/net/tcp6",
        };
        owner_in(&std::fs::read_to_string(table).ok()?, local, peer)
    }

    /// The owner of the client socket (local address `peer`, remote address `local`).
    pub(super) fn owner_in(table: &str, local: SocketAddr, peer: SocketAddr) -> Option<u32> {
        table.lines().skip(1).find_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            let (from, to, uid) = (cols.get(1)?, cols.get(2)?, cols.get(7)?);
            (parse(from)? == peer && parse(to)? == local).then(|| uid.parse().ok())?
        })
    }

    /// `0100007F:1F90`: the address as the kernel's words in host order, then the port.
    fn parse(field: &str) -> Option<SocketAddr> {
        let (addr, port) = field.split_once(':')?;
        let port = u16::from_str_radix(port, 16).ok()?;
        let mut bytes = Vec::with_capacity(16);
        for word in addr.as_bytes().chunks(8) {
            let word = u32::from_str_radix(std::str::from_utf8(word).ok()?, 16).ok()?;
            bytes.extend_from_slice(&word.to_ne_bytes());
        }
        let ip = match bytes.len() {
            4 => IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(bytes).ok()?)),
            16 => IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(bytes).ok()?)),
            _ => return None,
        };
        Some(SocketAddr::new(ip, port))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod imp {
    //! No way to look up the client here: serve it, as `kubectl port-forward` does.

    use std::net::SocketAddr;

    pub(super) fn owner(_local: SocketAddr, _peer: SocketAddr) -> Option<u32> {
        Some(super::current_uid())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};

    use super::same_user;

    fn connection(listener: &TcpListener) -> (TcpStream, SocketAddr, SocketAddr) {
        let local = listener.local_addr().unwrap();
        let client = TcpStream::connect(local).unwrap();
        let (_server, peer) = listener.accept().unwrap();
        (client, local, peer)
    }

    #[test]
    fn a_connection_from_this_user_is_served() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let (_client, local, peer) = connection(&listener);
        assert!(same_user(local, peer));
    }

    #[test]
    fn a_connection_from_this_user_over_ipv6_is_served() {
        let Ok(listener) = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)) else {
            return; // no IPv6 loopback on this machine
        };
        let (_client, local, peer) = connection(&listener);
        assert!(same_user(local, peer));
    }

    #[test]
    fn a_client_that_cannot_be_found_is_refused() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let (client, local, peer) = connection(&listener);
        // Another connection's ports, swapped: no socket goes from `local` to `peer`.
        assert!(!same_user(peer, local));
        drop(client);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_client_of_another_user_is_refused() {
        let local: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let peer: SocketAddr = "127.0.0.1:51000".parse().unwrap();
        let word = u32::from_ne_bytes([127, 0, 0, 1]);
        let other = super::current_uid().wrapping_add(1);
        let table = format!(
            "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   \
             0: {word:08X}:C738 {word:08X}:1F90 01 00000000:00000000 00:00000000 00000000 {other:>5}        0 4242 1\n"
        );
        assert_eq!(super::imp::owner_in(&table, local, peer), Some(other));
    }
}
