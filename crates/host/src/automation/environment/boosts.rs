use super::*;
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Boosts {
    next: Option<Instant>,
    cycle: Option<Cycle>,
    lease: Option<(Lease, Instant)>,
    issued: bool,
    settling: Vec<(Lease, Instant)>,
}
enum Cycle {
    Snapshot(Read<Snapshot>),
    Select {
        items: VecDeque<ItemData>,
        pending: Option<(ItemData, Read<Value>)>,
        failed: bool,
    },
    Prepare(ItemData, Read<Snapshot>, bool),
    Member(ItemData, Read<Value>, bool),
    Use(ItemData, Read<Value>, bool),
    Quantity {
        item: ItemData,
        read: Option<Read<Snapshot>>,
        poll: Instant,
        deadline: Instant,
        failed: bool,
    },
}
fn boost_type(link: &str) -> Option<&'static str> {
    match link.split("::").next()?.to_ascii_lowercase().as_str() {
        "xpboost" => Some("exp"),
        "gboost" => Some("gold"),
        "repboost" => Some("rep"),
        "cpboost" => Some("classPoints"),
        _ => None,
    }
}
impl Boosts {
    pub fn configure(&mut self, enabled: bool, now: Instant) {
        if !enabled {
            self.reset();
        } else if self.next.is_none() && self.cycle.is_none() {
            self.next = Some(now + BOOST_INTERVAL);
        }
    }
    pub fn reset(&mut self) {
        let mut settling = std::mem::take(&mut self.settling);
        if self.issued
            && let Some(lease) = self.lease.take()
        {
            settling.push(lease);
        }
        *self = Self {
            settling,
            ..Default::default()
        };
    }
    fn finish(&mut self, logs: &mut Failures, success: bool, now: Instant) {
        self.lease = None;
        self.issued = false;
        logs.set("boosts", success);
        self.next = Some(now + BOOST_INTERVAL);
    }
    pub fn advance(
        &mut self,
        host: &impl Host,
        waker: &Waker,
        config: &EnvironmentState,
        logs: &mut Failures,
        now: Instant,
    ) -> Option<Instant> {
        self.settling.retain(|(_, at)| now < *at);
        let cycle = self.advance_cycle(host, waker, config, logs, now);
        cycle
            .into_iter()
            .chain(self.settling.iter().map(|(_, at)| *at))
            .min()
    }
    fn advance_cycle(
        &mut self,
        host: &impl Host,
        waker: &Waker,
        config: &EnvironmentState,
        logs: &mut Failures,
        now: Instant,
    ) -> Option<Instant> {
        for _ in 0..64 {
            let Some(cycle) = self.cycle.take() else {
                let at = self.next?;
                if now < at {
                    return Some(at);
                }
                self.next = None;
                self.cycle = Some(Cycle::Snapshot(Read::new(host.snapshot(false), waker, now)));
                continue;
            };
            match cycle {
                Cycle::Snapshot(read) => match read.poll(now) {
                    None => {
                        let at = read.deadline;
                        self.cycle = Some(Cycle::Snapshot(read));
                        return Some(at);
                    }
                    Some(Err(_)) => self.finish(logs, false, now),
                    Some(Ok(snapshot)) => {
                        if !snapshot.logged_in {
                            self.next = Some(now + BOOST_INTERVAL);
                            continue;
                        }
                        let items = config
                            .boosts
                            .iter()
                            .filter_map(|name| {
                                snapshot
                                    .items
                                    .iter()
                                    .find(|item| item.name.to_lowercase() == name.to_lowercase())
                            })
                            .filter(|item| {
                                item.category == "ServerUse" && boost_type(&item.link).is_some()
                            })
                            .cloned()
                            .collect();
                        self.cycle = Some(Cycle::Select {
                            items,
                            pending: None,
                            failed: false,
                        });
                    }
                },
                Cycle::Select {
                    mut items,
                    mut pending,
                    mut failed,
                } => {
                    if let Some((item, read)) = pending.take() {
                        match read.poll(now) {
                            None => {
                                let at = read.deadline;
                                self.cycle = Some(Cycle::Select {
                                    items,
                                    pending: Some((item, read)),
                                    failed,
                                });
                                return Some(at);
                            }
                            Some(result) => match result.and_then(boolean) {
                                Ok(false)
                                    if config.boosts.iter().any(|name| {
                                        name.to_lowercase() == item.name.to_lowercase()
                                    }) =>
                                {
                                    self.cycle = Some(Cycle::Prepare(
                                        item,
                                        Read::new(host.snapshot(false), waker, now),
                                        failed,
                                    ));
                                    continue;
                                }
                                Err(_) => {
                                    failed = true;
                                    logs.set("boosts", false);
                                }
                                Ok(_) => {}
                            },
                        }
                    }
                    self.lease = None;
                    if let Some(item) = items.pop_front() {
                        match host.use_claim(item.item_id) {
                            Ok(Some(lease)) => self.lease = Some((lease, now + TIMEOUT)),
                            Ok(None) => {
                                self.cycle = Some(Cycle::Select {
                                    items,
                                    pending: None,
                                    failed,
                                });
                                continue;
                            }
                            Err(_) => {
                                logs.set("boosts", false);
                                failed = true;
                                self.cycle = Some(Cycle::Select {
                                    items,
                                    pending: None,
                                    failed,
                                });
                                continue;
                            }
                        }

                        let read = call(
                            host,
                            waker,
                            now,
                            "player.hasActiveBoost",
                            vec![Value::String(boost_type(&item.link).unwrap().into())],
                        );
                        self.cycle = Some(Cycle::Select {
                            items,
                            pending: Some((item, read)),
                            failed,
                        });
                    } else {
                        self.finish(logs, !failed, now);
                    }
                }
                Cycle::Prepare(selected, read, failed) => match read.poll(now) {
                    None => {
                        let at = read.deadline;
                        self.cycle = Some(Cycle::Prepare(selected, read, failed));
                        return Some(at);
                    }
                    Some(Err(_)) => self.finish(logs, false, now),
                    Some(Ok(snapshot)) => {
                        if !snapshot.logged_in
                            || !config
                                .boosts
                                .iter()
                                .any(|name| name.to_lowercase() == selected.name.to_lowercase())
                        {
                            self.finish(logs, !failed, now);
                            continue;
                        }
                        let Some(item) = snapshot
                            .items
                            .into_iter()
                            .find(|item| item.item_id == selected.item_id)
                        else {
                            self.finish(logs, false, now);
                            continue;
                        };
                        if item.category != "ServerUse" || boost_type(&item.link).is_none() {
                            self.finish(logs, false, now);
                            continue;
                        }
                        let method = if item.member_only {
                            "player.isMember"
                        } else {
                            "inventory.use"
                        };
                        let args = if item.member_only {
                            vec![]
                        } else {
                            vec![Value::Object(
                                vec![("itemId".into(), Value::Number(item.item_id as f64))]
                                    .into_iter()
                                    .collect(),
                            )]
                        };
                        self.issued = !item.member_only;
                        if self.issued
                            && let Some((_, at)) = &mut self.lease
                        {
                            *at = now + TIMEOUT;
                        }
                        let read = call(host, waker, now, method, args);
                        self.cycle = Some(if item.member_only {
                            Cycle::Member(item, read, failed)
                        } else {
                            Cycle::Use(item, read, failed)
                        });
                    }
                },
                Cycle::Member(item, read, failed) => match read.poll(now) {
                    None => {
                        let at = read.deadline;
                        self.cycle = Some(Cycle::Member(item, read, failed));
                        return Some(at);
                    }
                    Some(result) => {
                        if !config
                            .boosts
                            .iter()
                            .any(|name| name.to_lowercase() == item.name.to_lowercase())
                        {
                            self.finish(logs, !failed, now);
                        } else if result.and_then(boolean) == Ok(true) {
                            let read = call(
                                host,
                                waker,
                                now,
                                "inventory.use",
                                vec![Value::Object(
                                    vec![("itemId".into(), Value::Number(item.item_id as f64))]
                                        .into_iter()
                                        .collect(),
                                )],
                            );
                            self.issued = true;
                            if let Some((_, at)) = &mut self.lease {
                                *at = now + TIMEOUT;
                            }
                            self.cycle = Some(Cycle::Use(item, read, failed));
                        } else {
                            self.finish(logs, false, now);
                        }
                    }
                },
                Cycle::Use(item, read, failed) => match read.poll(now) {
                    None => {
                        let at = read.deadline;
                        self.cycle = Some(Cycle::Use(item, read, failed));
                        return Some(at);
                    }
                    Some(result) => {
                        if result.and_then(boolean) == Ok(true) {
                            if let Some((_, at)) = &mut self.lease {
                                *at = now + TIMEOUT;
                            }
                            self.cycle = Some(Cycle::Quantity {
                                item,
                                read: None,
                                poll: now,
                                deadline: now + TIMEOUT,
                                failed,
                            });
                        } else {
                            self.finish(logs, false, now);
                        }
                    }
                },
                Cycle::Quantity {
                    item,
                    mut read,
                    poll,
                    deadline,
                    failed,
                } => {
                    if now >= deadline {
                        self.finish(logs, false, now);
                        continue;
                    }
                    if now < poll {
                        self.cycle = Some(Cycle::Quantity {
                            item,
                            read,
                            poll,
                            deadline,
                            failed,
                        });
                        return Some(poll.min(deadline));
                    }
                    if read.is_none() {
                        read = Some(Read::new(host.snapshot(false), waker, now));
                    }
                    match read.as_ref().unwrap().poll(now) {
                        None => {
                            let at = read.as_ref().unwrap().deadline.min(deadline);
                            self.cycle = Some(Cycle::Quantity {
                                item,
                                read,
                                poll,
                                deadline,
                                failed,
                            });
                            return Some(at);
                        }
                        Some(Ok(snapshot))
                            if snapshot
                                .items
                                .iter()
                                .find(|i| i.item_id == item.item_id)
                                .is_none_or(|i| i.quantity < item.quantity) =>
                        {
                            self.finish(logs, !failed, now)
                        }
                        Some(Ok(_)) => {
                            self.cycle = Some(Cycle::Quantity {
                                item,
                                read: None,
                                poll: now + POLL,
                                deadline,
                                failed,
                            });
                            return Some((now + POLL).min(deadline));
                        }
                        Some(Err(_)) => self.finish(logs, false, now),
                    }
                }
            }
        }
        Some(now)
    }
}
