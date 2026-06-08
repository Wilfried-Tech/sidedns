use async_trait::async_trait;
use sidedns_core::{
    DnsEvent, DnsRule, IpcRequest, IpcResponse,
    ipc::{IpcClient, IpcHandler, IpcServer},
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn sock() -> String {
    #[cfg(windows)]
    return format!(r"\\.\pipe\sd-test-{}", Uuid::new_v4());
    #[cfg(not(windows))]
    format!("/tmp/sd-test-{}.sock", Uuid::new_v4())
}
fn addr(p: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p)
}
fn rule(domain: &str, port: u16) -> DnsRule {
    DnsRule::new(domain.to_string(), addr(port), false)
}

struct Mock {
    rules: RwLock<Vec<DnsRule>>,
    events: broadcast::Sender<DnsEvent>,
    token: CancellationToken,
}
impl Mock {
    fn new(token: CancellationToken) -> Arc<Self> {
        let (events, _) = broadcast::channel(32);
        Arc::new(Self {
            rules: RwLock::new(vec![]),
            events,
            token,
        })
    }
}
#[async_trait]
impl IpcHandler for Mock {
    async fn handle(&self, req: IpcRequest) -> IpcResponse {
        match req {
            IpcRequest::Add {
                domain,
                target,
                https,
            } => {
                let r = DnsRule::new(domain.clone(), target, https);
                let mut rules = self.rules.write().await;
                rules.retain(|x| x.domain != domain);
                rules.push(r.clone());
                let _ = self.events.send(DnsEvent::RuleAdded(r));
                IpcResponse::Ok
            },
            IpcRequest::Remove { domain } => {
                let mut rules = self.rules.write().await;
                let before = rules.len();
                rules.retain(|r| r.domain != domain);
                if rules.len() < before {
                    let _ = self.events.send(DnsEvent::RuleRemoved(rule(&domain, 0)));
                    IpcResponse::Ok
                } else {
                    IpcResponse::Error("not found".into())
                }
            },
            IpcRequest::List => IpcResponse::Rules(self.rules.read().await.clone()),
            IpcRequest::Resolve { domain } => IpcResponse::Resolved(
                self.rules
                    .read()
                    .await
                    .iter()
                    .find(|r| r.matches(&domain))
                    .cloned(),
            ),
            IpcRequest::Status => IpcResponse::Status {
                running: true,
                rule_count: self.rules.read().await.len(),
            },
            IpcRequest::Stop => {
                self.token.cancel();
                IpcResponse::Ok
            },
            _ => IpcResponse::Error("not implemented".into()),
        }
    }
    fn subscribe_events(&self) -> broadcast::Receiver<DnsEvent> {
        self.events.subscribe()
    }
}

async fn start(path: &str) -> (Arc<Mock>, CancellationToken) {
    let token = CancellationToken::new();
    let handler = Mock::new(token.clone());
    let server = IpcServer::with_path(path);
    let (h, t) = (handler.clone(), token.clone());
    tokio::spawn(async move { server.serve(h, t).await.unwrap() });
    let c = IpcClient::with_path(path);
    for _ in 0..30 {
        if c.is_running().await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    (handler, token)
}

#[tokio::test]
async fn is_running_false_when_absent() {
    assert!(
        !IpcClient::with_path(format!("/tmp/sd-absent-{}.sock", Uuid::new_v4()))
            .is_running()
            .await
    );
}
#[tokio::test]
async fn is_running_true_when_active() {
    let p = sock();
    let (_, t) = start(&p).await;
    assert!(IpcClient::with_path(&p).is_running().await);
    t.cancel();
}
#[tokio::test]
async fn add_returns_ok() {
    let p = sock();
    let (_, t) = start(&p).await;
    assert_eq!(
        IpcClient::with_path(&p)
            .send(IpcRequest::Add {
                domain: "api.local".into(),
                target: addr(3000),
                https: false
            })
            .await
            .unwrap(),
        IpcResponse::Ok
    );
    t.cancel();
}
#[tokio::test]
async fn list_empty_initially() {
    let p = sock();
    let (_, t) = start(&p).await;
    let IpcResponse::Rules(r) = IpcClient::with_path(&p)
        .send(IpcRequest::List)
        .await
        .unwrap()
    else {
        panic!();
    };
    assert!(r.is_empty());
    t.cancel();
}
#[tokio::test]
async fn list_returns_added_rules() {
    let p = sock();
    let (_, t) = start(&p).await;
    let c = IpcClient::with_path(&p);
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(3000),
        https: false,
    })
    .await
    .unwrap();
    c.send(IpcRequest::Add {
        domain: "auth.local".into(),
        target: addr(4000),
        https: false,
    })
    .await
    .unwrap();
    let IpcResponse::Rules(r) = c.send(IpcRequest::List).await.unwrap() else {
        panic!();
    };
    assert_eq!(r.len(), 2);
    t.cancel();
}
#[tokio::test]
async fn add_replaces_same_domain() {
    let p = sock();
    let (_, t) = start(&p).await;
    let c = IpcClient::with_path(&p);
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(3000),
        https: false,
    })
    .await
    .unwrap();
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(9000),
        https: false,
    })
    .await
    .unwrap();
    let IpcResponse::Rules(r) = c.send(IpcRequest::List).await.unwrap() else {
        panic!();
    };
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].target.port(), 9000);
    t.cancel();
}
#[tokio::test]
async fn remove_existing_ok() {
    let p = sock();
    let (_, t) = start(&p).await;
    let c = IpcClient::with_path(&p);
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(3000),
        https: false,
    })
    .await
    .unwrap();
    assert_eq!(
        c.send(IpcRequest::Remove {
            domain: "api.local".into()
        })
        .await
        .unwrap(),
        IpcResponse::Ok
    );
    let IpcResponse::Rules(r) = c.send(IpcRequest::List).await.unwrap() else {
        panic!();
    };
    assert!(r.is_empty());
    t.cancel();
}
#[tokio::test]
async fn remove_unknown_error() {
    let p = sock();
    let (_, t) = start(&p).await;
    assert!(matches!(
        IpcClient::with_path(&p)
            .send(IpcRequest::Remove {
                domain: "ghost.local".into()
            })
            .await
            .unwrap(),
        IpcResponse::Error(_)
    ));
    t.cancel();
}
#[tokio::test]
async fn resolve_known() {
    let p = sock();
    let (_, t) = start(&p).await;
    let c = IpcClient::with_path(&p);
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(9090),
        https: false,
    })
    .await
    .unwrap();
    let IpcResponse::Resolved(Some(r)) = c
        .send(IpcRequest::Resolve {
            domain: "api.local".into(),
        })
        .await
        .unwrap()
    else {
        panic!();
    };
    assert_eq!(r.target.port(), 9090);
    t.cancel();
}
#[tokio::test]
async fn resolve_unknown_none() {
    let p = sock();
    let (_, t) = start(&p).await;
    let IpcResponse::Resolved(r) = IpcClient::with_path(&p)
        .send(IpcRequest::Resolve {
            domain: "ghost.local".into(),
        })
        .await
        .unwrap()
    else {
        panic!();
    };
    assert!(r.is_none());
    t.cancel();
}
#[tokio::test]
async fn status_reports_count() {
    let p = sock();
    let (_, t) = start(&p).await;
    let c = IpcClient::with_path(&p);
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(3000),
        https: false,
    })
    .await
    .unwrap();
    let IpcResponse::Status {
        running,
        rule_count,
    } = c.send(IpcRequest::Status).await.unwrap()
    else {
        panic!();
    };
    assert!(running);
    assert_eq!(rule_count, 1);
    t.cancel();
}
#[tokio::test]
async fn stop_cancels_token() {
    let p = sock();
    let (_, t) = start(&p).await;
    IpcClient::with_path(&p)
        .send(IpcRequest::Stop)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(t.is_cancelled());
}
#[tokio::test]
async fn subscribe_receives_rule_added() {
    let p = sock();
    let (_, t) = start(&p).await;
    let c = IpcClient::with_path(&p);
    let mut rx = c.subscribe().await.unwrap();
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(3000),
        https: false,
    })
    .await
    .unwrap();
    let ev = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev, DnsEvent::RuleAdded(r) if r.domain == "api.local"));
    t.cancel();
}
#[tokio::test]
async fn subscribe_receives_rule_removed() {
    let p = sock();
    let (_, t) = start(&p).await;
    let c = IpcClient::with_path(&p);
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(3000),
        https: false,
    })
    .await
    .unwrap();
    let mut rx = c.subscribe().await.unwrap();
    c.send(IpcRequest::Remove {
        domain: "api.local".into(),
    })
    .await
    .unwrap();
    let ev = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev, DnsEvent::RuleRemoved(_)));
    t.cancel();
}
#[tokio::test]
async fn multiple_subscribers() {
    let p = sock();
    let (_, t) = start(&p).await;
    let c = IpcClient::with_path(&p);
    let mut rx1 = c.subscribe().await.unwrap();
    let mut rx2 = c.subscribe().await.unwrap();
    c.send(IpcRequest::Add {
        domain: "api.local".into(),
        target: addr(3000),
        https: false,
    })
    .await
    .unwrap();
    assert!(matches!(
        tokio::time::timeout(std::time::Duration::from_secs(1), rx1.recv())
            .await
            .unwrap()
            .unwrap(),
        DnsEvent::RuleAdded(_)
    ));
    assert!(matches!(
        tokio::time::timeout(std::time::Duration::from_secs(1), rx2.recv())
            .await
            .unwrap()
            .unwrap(),
        DnsEvent::RuleAdded(_)
    ));
    t.cancel();
}
#[tokio::test]
async fn concurrent_clients() {
    let p = sock();
    let (_, t) = start(&p).await;
    let hs: Vec<_> = (0..8u16)
        .map(|i| {
            let p = p.clone();
            tokio::spawn(async move {
                IpcClient::with_path(&p)
                    .send(IpcRequest::Add {
                        domain: format!("s{i}.local"),
                        target: addr(3000 + i),
                        https: false,
                    })
                    .await
                    .unwrap()
            })
        })
        .collect();
    for h in hs {
        assert_eq!(h.await.unwrap(), IpcResponse::Ok);
    }
    let IpcResponse::Rules(r) = IpcClient::with_path(&p)
        .send(IpcRequest::List)
        .await
        .unwrap()
    else {
        panic!();
    };
    assert_eq!(r.len(), 8);
    t.cancel();
}

// Serde round-trips
#[test]
fn request_serde() {
    for req in [
        IpcRequest::Add {
            domain: "api.local".into(),
            target: addr(3000),
            https: false,
        },
        IpcRequest::Add {
            domain: "api.local".into(),
            target: addr(443),
            https: true,
        },
        IpcRequest::Remove {
            domain: "api.local".into(),
        },
        IpcRequest::List,
        IpcRequest::Status,
        IpcRequest::Stop,
        IpcRequest::Subscribe,
        IpcRequest::Resolve {
            domain: "api.local".into(),
        },
    ] {
        let j = serde_json::to_string(&req).unwrap();
        let back: IpcRequest = serde_json::from_str(&j).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), j);
    }
}
#[test]
fn response_serde() {
    for resp in [
        IpcResponse::Ok,
        IpcResponse::Error("err".into()),
        IpcResponse::Rules(vec![rule("api.local", 3000)]),
        IpcResponse::Resolved(Some(rule("api.local", 3000))),
        IpcResponse::Resolved(None),
        IpcResponse::Status {
            running: true,
            rule_count: 5,
        },
    ] {
        let j = serde_json::to_string(&resp).unwrap();
        let back: IpcResponse = serde_json::from_str(&j).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), j);
    }
}
#[test]
fn event_serde() {
    for ev in [
        DnsEvent::RuleAdded(rule("api.local", 3000)),
        DnsEvent::RuleRemoved(rule("api.local", 3000)),
        DnsEvent::EphemeralAdded(rule("api.local", 3000)),
        DnsEvent::EphemeralRemoved(rule("api.local", 3000)),
        DnsEvent::DaemonStopped,
    ] {
        let j = serde_json::to_string(&ev).unwrap();
        let back: DnsEvent = serde_json::from_str(&j).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), j);
    }
}
