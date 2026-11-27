use super::*;
use horizon_client::items::ItemStack;
use horizon_protocol::packets::play::OpenScreen;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const SCRUB_FENCE_CHANNEL: &str = "rreturn:fence";
const SCRUB_DRAIN_TIMEOUT: Duration = Duration::from_millis(250);

fn is_fence_packet(pkt: &C, expected: &[u8; 8]) -> bool {
    matches!(
        pkt,
        C::CustomPayload(p) if p.channel == SCRUB_FENCE_CHANNEL && p.data.as_slice() == expected
    )
}

fn should_keep_discarded_event(ev: &Event) -> bool {
    matches!(
        ev,
        Event::Disconnected { .. }
            | Event::Reconfiguring
            | Event::Reconfigured(_)
            | Event::Transfer { .. }
            | Event::PackPush(_)
            | Event::PackPop(_)
    ) || matches!(
        ev,
        Event::Packet(pkt) if matches!(&**pkt, C::Ping(_) | C::Disconnect(_))
    )
}

thread_local! {
    static REMOTE_CRAFT_TABLE_POS: RefCell<Option<(i32, [i32; 3])>> = const { RefCell::new(None) };
}

fn clear_remote_craft_table_pos() {
    REMOTE_CRAFT_TABLE_POS.with(|c| {
        *c.borrow_mut() = None;
    });
}

pub trait RemoteExt {
    fn rreturn_reset_client_state(&mut self);
    fn rreturn_clear_remote_menu(&mut self);
    fn rreturn_clear_transient_menu_state(&mut self);
    fn rreturn_set_use_target(&mut self, target: Option<[i32; 3]>);
}

pub trait GameRemoteContainerExt {
    fn rreturn_remote_close_local_only(&mut self);
    fn rreturn_on_remote_open_screen_pre(&self, p: &OpenScreen);
    fn rreturn_craft_table_pos(&self) -> Option<[i32; 3]>;
    fn rreturn_craft_result(&self) -> Option<ItemStack>;
    fn rreturn_set_craft_result(&mut self, item: Option<ItemStack>);
    fn rreturn_sync_player_heads(&mut self, uuids: &[u128]) -> HashMap<u128, String>;
    fn rreturn_set_remote_chat_lines(&mut self, lines: &[String]);
    fn rreturn_sync_creative_cache(&mut self);
    fn rreturn_drain_remote_until_fence(&mut self, fence_nonce: Option<u64>, discard_world_updates: bool);
}

impl GameRemoteContainerExt for super::Game<'_> {
    fn rreturn_remote_close_local_only(&mut self) {
        clear_remote_craft_table_pos();
        self.remote_close_local();
    }

    fn rreturn_on_remote_open_screen_pre(&self, p: &OpenScreen) {
        let menu_name = self.remote.as_ref().and_then(|r| {
            r.builtin
                .get("minecraft:menu")?
                .name(p.menu)
                .map(str::to_owned)
        });
        if menu_name.as_deref() == Some("minecraft:crafting") {
            let pos = self.remote.as_ref().and_then(|r| {
                r.menu_extra
                    .as_ref()
                    .filter(|e| e.window == p.container)
                    .and_then(|e| e.pos)
                    .or(r.use_target)
            });
            REMOTE_CRAFT_TABLE_POS.with(|c| {
                *c.borrow_mut() = pos.map(|coords| (p.container, coords));
            });
        } else {
            clear_remote_craft_table_pos();
        }
    }

    fn rreturn_craft_table_pos(&self) -> Option<[i32; 3]> {
        if self.craft_table.is_some() {
            return self.craft_table;
        }
        if self.craft_size < 3 {
            return None;
        }
        let win = self
            .remote
            .as_ref()
            .and_then(|r| r.menu.as_ref())
            .filter(|(_, name)| name == "minecraft:crafting")
            .map(|(w, _)| *w)?;
        REMOTE_CRAFT_TABLE_POS.with(|c| {
            c.borrow()
                .filter(|(stored_w, _)| *stored_w == win)
                .map(|(_, pos)| pos)
        })
    }

    fn rreturn_craft_result(&self) -> Option<ItemStack> {
        self.remote.as_ref().and_then(|r| r.craft_result.clone())
    }

    fn rreturn_set_craft_result(&mut self, item: Option<ItemStack>) {
        if let Some(r) = self.remote.as_mut() {
            r.craft_result = item;
        }
    }

    fn rreturn_sync_player_heads(&mut self, uuids: &[u128]) -> HashMap<u128, String> {
        self.flush_skin_slots();
        let tab_skins: HashMap<u128, String> = self
            .remote_tab_players()
            .map(|(players, _, _)| players.into_iter().map(|p| (p.uuid, p.skin)).collect())
            .unwrap_or_default();
        let local_uuid = self.level.player_uuid.map_or(0, |u| {
            u.iter()
                .fold(0u128, |a, &w| (a << 32) | u128::from(w as u32))
        });

        let mut out = HashMap::with_capacity(uuids.len());
        for &uuid in uuids {
            let mut skin = if let Some(raw) = tab_skins.get(&uuid) {
                self.face_skin(uuid, raw)
            } else if uuid == local_uuid {
                if let Some((rgba, _)) = horizon_client::skins::offline_skin() {
                    const OFFLINE: &str = "horizon/offline_skin";
                    if !self.skin_slots.contains(OFFLINE)
                        || !self.gui.atlas.sprites.contains_key(OFFLINE)
                    {
                        self.store_skin(OFFLINE.into(), rgba.clone());
                    }
                    if self.gui.atlas.sprites.contains_key(OFFLINE) {
                        OFFLINE.to_owned()
                    } else {
                        horizon_client::tab_list::default_skin(uuid).to_owned()
                    }
                } else {
                    horizon_client::tab_list::default_skin(uuid).to_owned()
                }
            } else {
                horizon_client::tab_list::default_skin(uuid).to_owned()
            };
            if !self.gui.atlas.sprites.contains_key(&skin) {
                skin = horizon_client::tab_list::default_skin(uuid).to_owned();
            }
            out.insert(uuid, skin);
        }
        out
    }

    fn rreturn_set_remote_chat_lines(&mut self, lines: &[String]) {
        if let Some(r) = self.remote.as_mut() {
            let now = Instant::now();
            r.chat = lines.iter().map(|s| (s.clone(), now)).collect();
        }
    }

    fn rreturn_sync_creative_cache(&mut self) {
        self.remote_synced();
    }

    fn rreturn_drain_remote_until_fence(
        &mut self,
        fence_nonce: Option<u64>,
        discard_world_updates: bool,
    ) {
        let Some(_) = self.remote.as_ref() else {
            return;
        };
        let fence_bytes = fence_nonce.map(u64::to_le_bytes);
        let deadline = Instant::now() + SCRUB_DRAIN_TIMEOUT;
        let mut seen_fence = false;
        let mut post_fence: VecDeque<(Received, Instant)> = VecDeque::new();

        if discard_world_updates {
            let mut deferred_keep: VecDeque<(Event, _)> = VecDeque::new();
            let pre_backlog = self.remote.as_mut().map(|r| {
                r.backlog_since = None;
                std::mem::take(&mut r.backlog)
            });
            if let Some(mut pre) = pre_backlog {
                while let Some((item, _)) = pre.pop_front() {
                    match item {
                        Received::Chunk(_, _) => {
                            while Instant::now() < deadline {
                                match self.remote.as_ref().map(|r| r.decoder.next()) {
                                    Some(Next::Decoding) => {
                                        std::thread::sleep(Duration::from_millis(1));
                                    }
                                    Some(Next::Ready(_) | Next::Lost) | None => break,
                                }
                            }
                        }
                        Received::Event(ev, credit) => {
                            if !seen_fence
                                && let Some(expected) = fence_bytes.as_ref()
                                && matches!(&ev, Event::Packet(pkt) if is_fence_packet(pkt, expected))
                            {
                                seen_fence = true;
                                while let Some(tail) = pre.pop_front() {
                                    post_fence.push_back(tail);
                                }
                                break;
                            }
                            if should_keep_discarded_event(&ev) {
                                deferred_keep.push_back((ev, credit));
                            }
                        }
                    }
                }
            }

            while !seen_fence && Instant::now() < deadline {
                let batch = match self.remote.as_ref() {
                    Some(r) => r.session.poll(),
                    None => break,
                };
                if batch.is_empty() {
                    if fence_bytes.is_none() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                let now = Instant::now();
                for receipt in batch {
                    let (ev, credit) = receipt.into_parts();
                    if !seen_fence {
                        if let Some(expected) = fence_bytes.as_ref()
                            && matches!(&ev, Event::Packet(pkt) if is_fence_packet(pkt, expected))
                        {
                            seen_fence = true;
                            continue;
                        }
                        if should_keep_discarded_event(&ev) {
                            deferred_keep.push_back((ev, credit));
                        }
                    } else if let Some(r) = self.remote.as_ref() {
                        let received = match ev {
                            Event::Packet(p) if matches!(*p, C::LevelChunkWithLight(_)) => {
                                match r.decoder.submit(r.chunk_format.clone(), p) {
                                    None => Received::Chunk(r.chunk_format.clone(), credit),
                                    Some(p) => Received::Event(Event::Packet(p), credit),
                                }
                            }
                            event => Received::Event(event, credit),
                        };
                        post_fence.push_back((received, now));
                    }
                }
                if seen_fence || fence_bytes.is_none() {
                    break;
                }
            }

            if let Some(r) = self.remote.as_mut() {
                let keep_at = Instant::now();
                r.predictions.clear();
                r.backlog.clear();
                for (ev, credit) in deferred_keep {
                    r.backlog.push_back((Received::Event(ev, credit), keep_at));
                }
                r.backlog.extend(post_fence);
                r.backlog_since = r.backlog.front().map(|(_, t)| *t);
            }
            return;
        }

        loop {
            if let Some(r) = self.remote.as_mut() {
                let mut events = std::mem::take(&mut r.backlog);
                if !seen_fence {
                    r.receive_credited_backlog(&mut events);
                    if let Some(expected) = fence_bytes.as_ref()
                        && let Some(pos) = events.iter().position(|(item, _)| {
                            matches!(item, Received::Event(Event::Packet(pkt), _) if is_fence_packet(pkt, expected))
                        })
                    {
                        seen_fence = true;
                        let mut tail = events.split_off(pos);
                        let _ = tail.pop_front();
                        post_fence.extend(tail);
                    }
                }
                r.backlog_since = events.front().map(|(_, t)| *t);
                r.backlog = events;
            }
            while self.remote.as_ref().is_some_and(|r| !r.backlog.is_empty())
                && Instant::now() < deadline
            {
                let _ = self.poll_remote_events();
                if self.remote.as_ref().is_some_and(|r| !r.backlog.is_empty()) {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            let backlog_empty = self.remote.as_ref().is_none_or(|r| r.backlog.is_empty());
            if ((fence_bytes.is_none() || seen_fence) && backlog_empty)
                || Instant::now() >= deadline
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        if let Some(r) = self.remote.as_mut() {
            r.backlog.extend(post_fence);
            r.backlog_since = r.backlog.front().map(|(_, t)| *t);
        }
    }
}

impl RemoteExt for Remote {
    fn rreturn_set_use_target(&mut self, target: Option<[i32; 3]>) {
        self.use_target = target;
    }

    fn rreturn_clear_remote_menu(&mut self) {
        clear_remote_craft_table_pos();
        self.menu = ClientMenu::default();
        self.craft_result = None;
        self.menu_title = None;
        self.menu_extra = None;
    }

    fn rreturn_clear_transient_menu_state(&mut self) {
        clear_remote_craft_table_pos();
        self.menu = ClientMenu::default();
        self.craft_result = None;
        self.menu_title = None;
        self.menu_extra = None;
        self.use_target = None;
        self.pending_keys.clear();
    }

    fn rreturn_reset_client_state(&mut self) {
        clear_remote_craft_table_pos();
        self.camera = None;
        self.use_item = None;
        self.predictions.clear();
        self.menu = ClientMenu::default();
        self.craft_result = None;
        self.menu_title = None;
        self.use_target = None;
        self.menu_extra = None;
        self.lerps.clear();
        self.cart_lerps.clear();
        self.stepped.clear();
        self.head_lerps.clear();
        self.ghast_dismount = None;
        self.pending_items.clear();
        self.flash_on_set_health = false;
        self.destroying = None;
        self.destroying_item = None;
        self.is_destroying = false;
        self.destroy_delay = 0;
        self.pending_keys.clear();
        self.swap_answer = [false; 2];
        self.stop_mining = false;
        self.placing = None;
        self.death_message = None;
        self.splash_sound = Default::default();
    }
}
