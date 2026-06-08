use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
};

use sysinfo::System;

const RETRY_INTERVAL_MS: u32 = 100;

/// Wait until ip:port is listening, with timeout.
pub async fn wait_for_port(ip: IpAddr, port: u16, timeout: u32) -> bool {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout.into());

    while tokio::time::Instant::now() < deadline {
        if tokio::net::TcpListener::bind(SocketAddr::new(ip, port))
            .await
            .is_err()
        {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(RETRY_INTERVAL_MS.into())).await;
    }
    false
}

pub fn detect_network_processes(process_pid: u32) -> Vec<(String, SocketAddr)> {
    let mut system = System::new_all();
    let mut target_pids = HashSet::new();
    let mut pids_to_check = vec![process_pid];

    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    while let Some(current_ppid) = pids_to_check.pop() {
        target_pids.insert(current_ppid);

        for (pid, process) in system.processes() {
            if let Some(ppid) = process.parent() {
                let ppid_u32 = ppid.as_u32();
                if ppid_u32 == current_ppid && !target_pids.contains(&pid.as_u32()) {
                    pids_to_check.push(pid.as_u32());
                }
            }
        }
    }

    if let Ok(all_listeners) = listeners::get_all() {
        return all_listeners
            .iter()
            .filter_map(|l| {
                if target_pids.contains(&l.process.pid) && l.protocol == listeners::Protocol::TCP
                // && l.socket.is_ipv4()
                {
                    Some((l.process.name.clone(), l.socket))
                } else {
                    None
                }
            })
            .collect();
    }
    vec![]
}
