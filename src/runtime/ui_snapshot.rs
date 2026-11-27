use super::super::{
    chat::ChatDraft,
    game_mode::mode_from_id,
    Screen, Game,
};
use super::policy::RollbackPolicy;
use super::scrub::is_scrubbing;
use super::scrub_hud::get_scrub_virtual_cursor;
use super::storage::slot_dir;
use super::types::*;
use horizon_client::{
    chat::{self, ChatSource},
    containers::BlockEntity,
    game_mode_switcher::GameModeSwitcher,
    items::ItemStack,
    save::{item_from_nbt, item_to_nbt},
    text::Text,
};
use horizon_core::nbt::{self, Limits as NbtLimits};
use horizon_protocol::packets::play::{self, Clientbound as C};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessageSnapshot {
    pub component: serde_json::Value,
    pub source: u8,
    pub has_indicator: bool,
    pub added_tick: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClientUiSnapshot {
    pub screen: String,
    pub creative_tab_id: Option<String>,
    pub creative_selected: usize,
    pub creative_page: u32,
    pub creative_scroll: f32,
    pub creative_search: String,
    pub creative_search_cursor: usize,
    pub creative_search_highlight: usize,
    pub chat_input: String,
    pub chat_cursor: usize,
    pub chat_highlight: usize,
    pub chat_display: usize,
    pub chat_draft: Option<(String, bool)>,
    pub chat_draft_shown: bool,
    pub in_bed_chat: bool,
    pub has_chat_log: bool,
    pub chat_log: Vec<ChatMessageSnapshot>,
    pub chat_sent_recent: Vec<String>,
    pub carried: Option<ItemStack>,
    pub craft_grid: Vec<Option<ItemStack>>,
    pub craft_result: Option<ItemStack>,
    pub craft_size: usize,
    pub craft_table: Option<[i32; 3]>,
    pub container: Option<[i32; 3]>,
    pub open_container_slots: Option<([i32; 3], usize)>,
    pub ender_open: Option<[i32; 3]>,
    pub station: Option<StationUiSnapshot>,
    pub recipe_ui: RecipeUiSnapshot,
    pub recipe_book_open: [bool; 4],
    pub recipe_book_filtering: [bool; 4],
    pub mount_menu: Option<MountMenuSnapshot>,
    pub sign_edit: Option<SignEditorSnapshot>,
    pub book: Option<BookScreenSnapshot>,
    pub command_block: Option<CommandBlockUiSnapshot>,
    pub gamemode_switcher_selected: Option<i32>,
    pub perspective: u8,
}

#[derive(Default)]
struct ChatJournalState {
    entries: Vec<ChatMessageSnapshot>,
    last_received: u64,
    last_applied: Option<(Vec<ChatMessageSnapshot>, Vec<String>)>,
    ignored_hashes: Vec<u64>,
}

fn hash_chat_plain(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in text.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub fn mark_telemetry_chat_text(text: &str) {
    let h = hash_chat_plain(text);
    CHAT_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        if !j.ignored_hashes.contains(&h) {
            if j.ignored_hashes.len() >= 256 {
                j.ignored_hashes.remove(0);
            }
            j.ignored_hashes.push(h);
        }
    });
}

thread_local! {
    static CLIENT_UI_SLOTS: RefCell<HashMap<String, ClientUiSnapshot>> = RefCell::new(HashMap::new());
    static ACTIVE_WORLD_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static CHAT_JOURNAL: RefCell<ChatJournalState> = RefCell::new(ChatJournalState::default());
}

fn infer_chat_source_and_indicator(component: &serde_json::Value) -> (u8, bool) {
    let key = component.get("translate").and_then(|v| v.as_str());
    match key {
        Some(
            "chat.type.text"
            | "chat.type.announcement"
            | "chat.type.emote"
            | "commands.message.display.incoming",
        ) => (0, false),
        _ => (1, true),
    }
}

pub fn clear_chat_journal() {
    CHAT_JOURNAL.with(|cell| *cell.borrow_mut() = ChatJournalState::default());
}

pub fn reset_chat_restore_cache() {
    CHAT_JOURNAL.with(|cell| cell.borrow_mut().last_applied = None);
}

pub fn sync_chat_journal(game: &Game<'_>) {
    let received = game.ui.chat.received;
    let empty_now = game.ui.chat.recent_components(1).is_empty();
    let needs_sync = CHAT_JOURNAL.with(|cell| {
        let j = cell.borrow();
        received != j.last_received || (empty_now && !j.entries.is_empty())
    });
    if !needs_sync {
        return;
    }

    let raw_live = game.ui.chat.recent_components(100);
    let now_ticks = game.visual_ticks();
    CHAT_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        let live: Vec<serde_json::Value> = raw_live
            .into_iter()
            .rev()
            .filter(|comp| {
                let plain = Text::from(comp.clone()).plain();
                let h = hash_chat_plain(&plain);
                !j.ignored_hashes.contains(&h)
            })
            .collect();
        if live.is_empty() {
            j.entries.clear();
            j.last_received = received;
            return;
        }
        let mut matched_entries = Vec::with_capacity(live.len());
        let old_len = j.entries.len();
        let mut matched_len = 0usize;
        for drop_old in 0..=old_len {
            let keep = old_len - drop_old;
            if keep <= live.len()
                && j.entries[drop_old..]
                    .iter()
                    .enumerate()
                    .all(|(i, prev)| prev.component == live[i])
            {
                matched_entries.extend(j.entries[drop_old..].iter().cloned());
                matched_len = keep;
                break;
            }
        }
        for comp in &live[matched_len..] {
            let (source, has_indicator) = infer_chat_source_and_indicator(comp);
            matched_entries.push(ChatMessageSnapshot {
                component: comp.clone(),
                source,
                has_indicator,
                added_tick: now_ticks,
            });
        }
        j.entries = matched_entries;
        j.last_received = received;
    });
}

pub fn apply_chat_history_from_snapshot(
    game: &mut Game<'_>,
    ui: &ClientUiSnapshot,
    force: bool,
) {
    if !ui.has_chat_log {
        return;
    }
    let same = if force {
        false
    } else {
        CHAT_JOURNAL.with(|cell| {
            let j = cell.borrow();
            j.last_applied
                .as_ref()
                .is_some_and(|(l, r)| l == &ui.chat_log && r == &ui.chat_sent_recent)
        })
    };
    if same {
        return;
    }

    let prev_log = game.ui.chat.log;
    game.ui.chat.log = false;
    game.ui.chat.clear();
    let mut plain_lines = Vec::with_capacity(ui.chat_log.len());
    for entry in &ui.chat_log {
        let source = match entry.source {
            0 => ChatSource::Player,
            2 => ChatSource::Client,
            _ => ChatSource::Server,
        };
        let indicator = entry.has_indicator.then_some(chat::SYSTEM_INDICATOR);
        let text = Text::from(entry.component.clone());
        plain_lines.push(text.plain());
        game.ui.chat.add(
            &game.gui.atlas,
            text,
            indicator,
            source,
            entry.added_tick,
            false,
        );
    }
    game.ui.chat.log = prev_log;
    game.ui.chat.recent = ui.chat_sent_recent.clone();
    game.rreturn_set_remote_chat_lines(&plain_lines);

    let received = game.ui.chat.received;
    CHAT_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        j.entries = ui.chat_log.clone();
        j.last_received = received;
        j.last_applied = Some((ui.chat_log.clone(), ui.chat_sent_recent.clone()));
    });
}

pub fn set_active_ui_world_root(world_root: &Path) {
    if world_root.as_os_str().is_empty() {
        return;
    }
    ACTIVE_WORLD_ROOT.with(|c| {
        *c.borrow_mut() = Some(world_root.to_path_buf());
    });
}

pub fn clear_active_ui_world_root() {
    ACTIVE_WORLD_ROOT.with(|c| {
        *c.borrow_mut() = None;
    });
}

pub fn resolve_ui_world_root(game_world_root: &Path) -> PathBuf {
    if !game_world_root.as_os_str().is_empty() {
        return game_world_root.to_path_buf();
    }
    ACTIVE_WORLD_ROOT.with(|c| c.borrow().clone().unwrap_or_default())
}

static UI_MODE_GUARD_TICKS: AtomicUsize = AtomicUsize::new(0);

pub fn arm_ui_mode_guard(ticks: usize) {
    let cur = UI_MODE_GUARD_TICKS.load(Ordering::Acquire);
    if ticks > cur {
        UI_MODE_GUARD_TICKS.store(ticks, Ordering::Release);
    }
}

pub fn tick_ui_mode_guard() {
    let cur = UI_MODE_GUARD_TICKS.load(Ordering::Acquire);
    if cur > 0 {
        UI_MODE_GUARD_TICKS.store(cur - 1, Ordering::Release);
    }
}

pub fn should_suppress_mode_inventory_reset() -> bool {
    UI_MODE_GUARD_TICKS.load(Ordering::Acquire) > 0 || is_scrubbing()
}

pub fn close_client_container_local(game: &mut Game<'_>) {
    game.pad_clear_focus();
    game.mod_browser_closed();
    game.reset_inventory_clicks();
    game.close_station();
    game.mount_closed();
    game.recipe_ui.reset_screen();
    if game.remote.is_some() {
        game.rreturn_remote_close_local_only();
    } else {
        game.carried = None;
        game.craft_grid.fill(None);
    }
    game.container = None;
    game.open_container_slots = None;
    game.ender_open = None;
    game.craft_table = None;
    game.craft_size = 2;
}

pub fn clear_client_ui_slots() {
    CLIENT_UI_SLOTS.with(|c| c.borrow_mut().clear());
}

pub fn client_screen_id(screen: Screen) -> &'static str {
    match screen {
        Screen::Playing => "playing",
        Screen::GameModeSwitcher => "game_mode_switcher",
        Screen::EmoteWheel => "emote_wheel",
        Screen::Paused => "paused",
        Screen::Options => "options",
        Screen::Chat => "chat",
        Screen::Creative => "creative",
        Screen::Inventory => "inventory",
        Screen::Dead => "dead",
        Screen::Stats => "stats",
        Screen::Advancements => "advancements",
        Screen::WorldOptions => "world_options",
        Screen::Friends => "friends",
        Screen::Social => "social",
        Screen::CommandBlock => "command_block",
        Screen::BlockEditor => "block_editor",
        Screen::ServerLinks => "server_links",
        Screen::ConfirmLink => "confirm_link",
        Screen::DialogWarning => "dialog_warning",
        Screen::DebugOptions => "debug_options",
        Screen::HudEditor => "hud_editor",
        Screen::Win => "win",
        Screen::Mod => "mod",
        Screen::Studio => "studio",
    }
}

pub fn is_scrub_recordable_client(game: &Game<'_>) -> bool {
    game.title.is_none()
        && game.player.is_some()
        && !game.pauses_game()
        && game.screen != Screen::Dead
}

pub fn is_text_input_focused(game: &Game<'_>) -> bool {
    if game.sign_edit.is_some() || game.rreturn_is_book_editing() {
        return true;
    }
    match game.screen {
        Screen::Chat | Screen::CommandBlock | Screen::Social => true,
        Screen::Creative => {
            game.creative_screen
                .tabs
                .get(game.creative_screen.selected)
                .is_some_and(|t| t.search)
                || game.mod_browser_typing()
        }
        Screen::Inventory => {
            game.recipe_search_focused() || game.station_typing() || game.mod_browser_typing()
        }
        Screen::Options | Screen::WorldOptions | Screen::DebugOptions | Screen::BlockEditor => true,
        _ => false,
    }
}

pub fn capture_client_ui_snapshot(game: &Game<'_>) -> ClientUiSnapshot {
    sync_chat_journal(game);
    let chat_log = CHAT_JOURNAL.with(|cell| cell.borrow().entries.clone());
    let chat_sent_recent = game.ui.chat.recent.clone();
    let creative_tab_id = game
        .creative_screen
        .tabs
        .get(game.creative_screen.selected)
        .map(|t| t.id.clone());
    let chat = game.ui.chat_screen.as_ref();
    ClientUiSnapshot {
        screen: client_screen_id(game.screen).to_owned(),
        creative_tab_id,
        creative_selected: game.creative_screen.selected,
        creative_page: game.creative_screen.page,
        creative_scroll: game.creative_screen.scroll,
        creative_search: game.creative_screen.search.edit.value.clone(),
        creative_search_cursor: game.creative_screen.search.edit.cursor,
        creative_search_highlight: game.creative_screen.search.edit.highlight,
        chat_input: chat.map(|c| c.input.value.clone()).unwrap_or_default(),
        chat_cursor: chat.map(|c| c.input.cursor).unwrap_or(0),
        chat_highlight: chat.map(|c| c.input.highlight).unwrap_or(0),
        chat_display: chat.map(|c| c.input.display).unwrap_or(0),
        chat_draft: game
            .ui
            .chat_draft
            .as_ref()
            .map(|d| (d.text.clone(), d.command)),
        chat_draft_shown: game.ui.chat_draft_shown,
        in_bed_chat: game.ui.in_bed_chat,
        has_chat_log: true,
        chat_log,
        chat_sent_recent,
        carried: game.carried.clone(),
        craft_grid: game.craft_grid.clone(),
        craft_result: game.rreturn_craft_result(),
        craft_size: game.craft_size,
        craft_table: game.rreturn_craft_table_pos(),
        container: game.container.map(|p| [p.0, p.1, p.2]),
        open_container_slots: game
            .open_container_slots
            .map(|(p, n)| ([p.0, p.1, p.2], n)),
        ender_open: game.ender_open.map(|p| [p.0, p.1, p.2]),
        station: game.rreturn_capture_station_ui(),
        recipe_ui: game.rreturn_capture_recipe_ui(),
        recipe_book_open: game.level.recipe_book.open,
        recipe_book_filtering: game.level.recipe_book.filtering,
        mount_menu: game.rreturn_capture_mount_menu(),
        sign_edit: game.rreturn_capture_sign_edit(),
        book: game.rreturn_capture_book(),
        command_block: game.rreturn_capture_command_block_ui(),
        gamemode_switcher_selected: (game.screen == Screen::GameModeSwitcher)
            .then(|| game.gamemode_switcher.as_ref().map(|s| s.selected.id()))
            .flatten(),
        perspective: game.rreturn_camera_type(),
    }
}

pub fn ui_encode_item(stack: &Option<ItemStack>) -> serde_json::Value {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let Some(s) = stack else {
        return serde_json::Value::Null;
    };
    let tag = item_to_nbt(s);
    match nbt::write_network(&tag, NbtLimits::saved()) {
        Ok(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for b in bytes {
                hex.push(HEX[(b >> 4) as usize] as char);
                hex.push(HEX[(b & 0x0f) as usize] as char);
            }
            serde_json::Value::String(hex)
        }
        Err(_) => serde_json::Value::Null,
    }
}

pub fn ui_decode_item(val: &serde_json::Value) -> Option<ItemStack> {
    let hex = val.as_str()?;
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    let (tag, _) = nbt::read_network(&bytes, NbtLimits::saved()).ok()?;
    item_from_nbt(&tag)
}

pub fn ui_encode_items(items: &[Option<ItemStack>]) -> Vec<serde_json::Value> {
    items.iter().map(ui_encode_item).collect()
}

pub fn ui_decode_items(val: &serde_json::Value) -> Vec<Option<ItemStack>> {
    val.as_array()
        .map(|arr| arr.iter().map(ui_decode_item).collect())
        .unwrap_or_default()
}

fn ui_encode_pos3(v: Option<[i32; 3]>) -> serde_json::Value {
    match v {
        Some(p) => serde_json::json!([p[0], p[1], p[2]]),
        None => serde_json::Value::Null,
    }
}

fn ui_decode_pos3(v: &serde_json::Value) -> Option<[i32; 3]> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    Some([
        a[0].as_i64()? as i32,
        a[1].as_i64()? as i32,
        a[2].as_i64()? as i32,
    ])
}

fn ui_station_to_json(st: &StationUiSnapshot) -> serde_json::Value {
    let clues: Vec<serde_json::Value> = st
        .enchant_clues
        .iter()
        .map(|c| match c {
            Some((id, lvl)) => serde_json::json!([id, lvl]),
            None => serde_json::Value::Null,
        })
        .collect();
    serde_json::json!({
        "kind": st.kind,
        "pos": st.pos,
        "inputs": ui_encode_items(&st.inputs),
        "result": ui_encode_item(&st.result),
        "anvil_name": st.anvil_name,
        "rename_cursor": st.rename_cursor,
        "rename_highlight": st.rename_highlight,
        "selected": st.selected,
        "start": st.start,
        "scroll": st.scroll,
        "beacon": [st.beacon[0], st.beacon[1]],
        "beacon_levels": st.beacon_levels,
        "loom_selected": st.loom_selected,
        "loom_scroll": st.loom_scroll,
        "loom_start_row": st.loom_start_row,
        "merchant_mob": st.merchant_mob,
        "merchant_scroll": st.merchant_scroll,
        "merchant_focused": st.merchant_focused,
        "enchant_costs": st.enchant_costs,
        "enchant_clues": clues,
    })
}

fn ui_station_from_json(v: &serde_json::Value) -> Option<StationUiSnapshot> {
    if v.is_null() {
        return None;
    }
    let kind = v["kind"].as_str()?.to_owned();
    let pos = ui_decode_pos3(&v["pos"])?;
    let mut clues: [Option<(String, u32)>; 3] = [None, None, None];
    if let Some(arr) = v["enchant_clues"].as_array() {
        for (i, item) in arr.iter().take(3).enumerate() {
            if let Some(pair) = item.as_array()
                && pair.len() == 2
                && let (Some(id), Some(lvl)) = (pair[0].as_str(), pair[1].as_u64())
            {
                clues[i] = Some((id.to_owned(), lvl as u32));
            }
        }
    }
    let mut costs = [0i32; 3];
    if let Some(arr) = v["enchant_costs"].as_array() {
        for (i, item) in arr.iter().take(3).enumerate() {
            costs[i] = item.as_i64().unwrap_or(0) as i32;
        }
    }
    let mut beacon = [None, None];
    if let Some(arr) = v["beacon"].as_array() {
        for (i, item) in arr.iter().take(2).enumerate() {
            beacon[i] = item.as_u64().map(|x| x as usize);
        }
    }
    Some(StationUiSnapshot {
        kind,
        pos,
        inputs: ui_decode_items(&v["inputs"]),
        result: ui_decode_item(&v["result"]),
        anvil_name: v["anvil_name"].as_str().unwrap_or("").to_owned(),
        rename_cursor: v["rename_cursor"].as_u64().unwrap_or(0) as usize,
        rename_highlight: v["rename_highlight"].as_u64().unwrap_or(0) as usize,
        selected: v["selected"].as_u64().map(|x| x as usize),
        start: v["start"].as_u64().unwrap_or(0) as usize,
        scroll: v["scroll"].as_f64().unwrap_or(0.0) as f32,
        beacon,
        beacon_levels: v["beacon_levels"].as_i64().map(|x| x as i32),
        loom_selected: v["loom_selected"].as_u64().map(|x| x as usize),
        loom_scroll: v["loom_scroll"].as_f64().unwrap_or(0.0) as f32,
        loom_start_row: v["loom_start_row"].as_u64().unwrap_or(0) as usize,
        merchant_mob: v["merchant_mob"].as_u64().map(|x| x as u32),
        merchant_scroll: v["merchant_scroll"].as_u64().unwrap_or(0) as usize,
        merchant_focused: v["merchant_focused"].as_u64().map(|x| x as usize),
        enchant_costs: costs,
        enchant_clues: clues,
    })
}

fn ui_snapshot_to_json(s: &ClientUiSnapshot) -> serde_json::Value {
    let ocs = s
        .open_container_slots
        .map(|(p, n)| serde_json::json!([p[0], p[1], p[2], n]))
        .unwrap_or(serde_json::Value::Null);
    let draft = s
        .chat_draft
        .as_ref()
        .map(|(t, c)| serde_json::json!([t, c]))
        .unwrap_or(serde_json::Value::Null);
    let mount = s.mount_menu.as_ref().map(|m| {
        serde_json::json!({
            "mob": m.mob,
            "columns": m.columns,
            "saddle": ui_encode_item(&m.saddle),
            "armor": ui_encode_item(&m.armor),
            "items": ui_encode_items(&m.items),
        })
    });
    let sign = s.sign_edit.as_ref().map(|se| {
        serde_json::json!({
            "pos": se.pos,
            "front": se.front,
            "line": se.line,
            "lines": se.lines,
            "edit_value": se.edit_value,
            "edit_cursor": se.edit_cursor,
            "edit_highlight": se.edit_highlight,
        })
    });
    let book = s.book.as_ref().map(|b| match b {
        BookScreenSnapshot::Edit {
            slot,
            pages,
            page,
            signing,
            title,
            title_value,
            title_cursor,
            title_highlight,
            page_value,
            page_cursor,
            page_highlight,
        } => serde_json::json!({
            "mode": "edit",
            "slot": slot,
            "pages": pages,
            "page": page,
            "signing": signing,
            "title": title,
            "title_value": title_value,
            "title_cursor": title_cursor,
            "title_highlight": title_highlight,
            "page_value": page_value,
            "page_cursor": page_cursor,
            "page_highlight": page_highlight,
        }),
        BookScreenSnapshot::View {
            page,
            lectern,
            plain_pages,
        } => serde_json::json!({
            "mode": "view",
            "page": page,
            "lectern": ui_encode_pos3(*lectern),
            "plain_pages": plain_pages,
        }),
    });
    let cb = s.command_block.as_ref().map(|c| {
        serde_json::json!({
            "pos": c.pos,
            "cart": c.cart,
            "command": c.command,
            "cursor": c.cursor,
            "highlight": c.highlight,
            "display": c.display,
            "track_output": c.track_output,
            "mode": c.mode,
            "conditional": c.conditional,
            "auto": c.auto,
        })
    });
    let chat_log: Vec<serde_json::Value> = s
        .chat_log
        .iter()
        .map(|e| {
            serde_json::json!({
                "component": e.component,
                "source": e.source,
                "has_indicator": e.has_indicator,
                "added_tick": e.added_tick,
            })
        })
        .collect();
    serde_json::json!({
        "screen": s.screen,
        "creative_tab_id": s.creative_tab_id,
        "creative_selected": s.creative_selected,
        "creative_page": s.creative_page,
        "creative_scroll": s.creative_scroll,
        "creative_search": s.creative_search,
        "creative_search_cursor": s.creative_search_cursor,
        "creative_search_highlight": s.creative_search_highlight,
        "chat_input": s.chat_input,
        "chat_cursor": s.chat_cursor,
        "chat_highlight": s.chat_highlight,
        "chat_display": s.chat_display,
        "chat_draft": draft,
        "chat_draft_shown": s.chat_draft_shown,
        "in_bed_chat": s.in_bed_chat,
        "has_chat_log": s.has_chat_log,
        "chat_log": chat_log,
        "chat_sent_recent": s.chat_sent_recent,
        "carried": ui_encode_item(&s.carried),
        "craft_grid": ui_encode_items(&s.craft_grid),
        "craft_result": ui_encode_item(&s.craft_result),
        "craft_size": s.craft_size,
        "craft_table": ui_encode_pos3(s.craft_table),
        "container": ui_encode_pos3(s.container),
        "open_container_slots": ocs,
        "ender_open": ui_encode_pos3(s.ender_open),
        "station": s.station.as_ref().map(ui_station_to_json).unwrap_or(serde_json::Value::Null),
        "recipe_ui": {
            "tab": s.recipe_ui.tab,
            "page": s.recipe_ui.page,
            "search_value": s.recipe_ui.search_value,
            "search_cursor": s.recipe_ui.search_cursor,
            "search_highlight": s.recipe_ui.search_highlight,
            "search_focused": s.recipe_ui.search_focused,
        },
        "recipe_book_open": s.recipe_book_open,
        "recipe_book_filtering": s.recipe_book_filtering,
        "mount_menu": mount,
        "sign_edit": sign,
        "book": book,
        "command_block": cb,
        "gamemode_switcher_selected": s.gamemode_switcher_selected,
        "perspective": s.perspective,
    })
}

fn ui_snapshot_from_json(v: &serde_json::Value) -> Option<ClientUiSnapshot> {
    let screen = v["screen"].as_str()?.to_owned();
    let open_container_slots = v["open_container_slots"].as_array().and_then(|a| {
        if a.len() == 4 {
            Some((
                [
                    a[0].as_i64()? as i32,
                    a[1].as_i64()? as i32,
                    a[2].as_i64()? as i32,
                ],
                a[3].as_u64()? as usize,
            ))
        } else {
            None
        }
    });
    let chat_draft = v["chat_draft"].as_array().and_then(|a| {
        if a.len() == 2 {
            Some((a[0].as_str()?.to_owned(), a[1].as_bool()?))
        } else {
            None
        }
    });
    let mut recipe_book_open = [false; 4];
    if let Some(arr) = v["recipe_book_open"].as_array() {
        for (i, x) in arr.iter().take(4).enumerate() {
            recipe_book_open[i] = x.as_bool().unwrap_or(false);
        }
    }
    let mut recipe_book_filtering = [false; 4];
    if let Some(arr) = v["recipe_book_filtering"].as_array() {
        for (i, x) in arr.iter().take(4).enumerate() {
            recipe_book_filtering[i] = x.as_bool().unwrap_or(false);
        }
    }
    let r = &v["recipe_ui"];
    let recipe_ui = RecipeUiSnapshot {
        tab: r["tab"].as_str().map(str::to_owned),
        page: r["page"].as_u64().unwrap_or(0) as usize,
        search_value: r["search_value"].as_str().unwrap_or("").to_owned(),
        search_cursor: r["search_cursor"].as_u64().unwrap_or(0) as usize,
        search_highlight: r["search_highlight"].as_u64().unwrap_or(0) as usize,
        search_focused: r["search_focused"].as_bool().unwrap_or(false),
    };
    let mount_menu = (!v["mount_menu"].is_null()).then(|| &v["mount_menu"]).and_then(|m| {
        Some(MountMenuSnapshot {
            mob: m["mob"].as_u64()? as u32,
            columns: m["columns"].as_u64()? as usize,
            saddle: ui_decode_item(&m["saddle"]),
            armor: ui_decode_item(&m["armor"]),
            items: ui_decode_items(&m["items"]),
        })
    });
    let sign_edit = (!v["sign_edit"].is_null()).then(|| &v["sign_edit"]).and_then(|se| {
        let arr = se["lines"].as_array()?;
        if arr.len() < 4 {
            return None;
        }
        Some(SignEditorSnapshot {
            pos: ui_decode_pos3(&se["pos"])?,
            front: se["front"].as_bool().unwrap_or(true),
            line: se["line"].as_u64().unwrap_or(0) as usize,
            lines: [
                arr[0].as_str().unwrap_or("").to_owned(),
                arr[1].as_str().unwrap_or("").to_owned(),
                arr[2].as_str().unwrap_or("").to_owned(),
                arr[3].as_str().unwrap_or("").to_owned(),
            ],
            edit_value: se["edit_value"].as_str().unwrap_or("").to_owned(),
            edit_cursor: se["edit_cursor"].as_u64().unwrap_or(0) as usize,
            edit_highlight: se["edit_highlight"].as_u64().unwrap_or(0) as usize,
        })
    });
    let book = (!v["book"].is_null()).then(|| &v["book"]).and_then(|b| {
        if b["mode"].as_str() == Some("edit") {
            let pages = b["pages"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
                .unwrap_or_default();
            Some(BookScreenSnapshot::Edit {
                slot: b["slot"].as_u64().unwrap_or(0) as usize,
                pages,
                page: b["page"].as_u64().unwrap_or(0) as usize,
                signing: b["signing"].as_bool().unwrap_or(false),
                title: b["title"].as_str().unwrap_or("").to_owned(),
                title_value: b["title_value"].as_str().unwrap_or("").to_owned(),
                title_cursor: b["title_cursor"].as_u64().unwrap_or(0) as usize,
                title_highlight: b["title_highlight"].as_u64().unwrap_or(0) as usize,
                page_value: b["page_value"].as_str().unwrap_or("").to_owned(),
                page_cursor: b["page_cursor"].as_u64().unwrap_or(0) as usize,
                page_highlight: b["page_highlight"].as_u64().unwrap_or(0) as usize,
            })
        } else if b["mode"].as_str() == Some("view") {
            let plain_pages = b["plain_pages"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
                .unwrap_or_default();
            Some(BookScreenSnapshot::View {
                page: b["page"].as_u64().unwrap_or(0) as usize,
                lectern: ui_decode_pos3(&b["lectern"]),
                plain_pages,
            })
        } else {
            None
        }
    });
    let command_block = (!v["command_block"].is_null())
        .then(|| &v["command_block"])
        .and_then(|c| {
            Some(CommandBlockUiSnapshot {
                pos: ui_decode_pos3(&c["pos"])?,
                cart: c["cart"].as_u64().map(|x| x as u32),
                command: c["command"].as_str().unwrap_or("").to_owned(),
                cursor: c["cursor"].as_u64().unwrap_or(0) as usize,
                highlight: c["highlight"].as_u64().unwrap_or(0) as usize,
                display: c["display"].as_u64().unwrap_or(0) as usize,
                track_output: c["track_output"].as_bool().unwrap_or(true),
                mode: c["mode"].as_u64().unwrap_or(2) as u8,
                conditional: c["conditional"].as_bool().unwrap_or(false),
                auto: c["auto"].as_bool().unwrap_or(false),
            })
        });
    let has_chat_log = v["has_chat_log"].as_bool().unwrap_or_else(|| v["chat_log"].is_array());
    let chat_log = v["chat_log"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let comp = item.get("component")?.clone();
                    if comp.is_null() {
                        return None;
                    }
                    Some(ChatMessageSnapshot {
                        component: comp,
                        source: item["source"].as_u64().unwrap_or(1) as u8,
                        has_indicator: item["has_indicator"].as_bool().unwrap_or(false),
                        added_tick: item["added_tick"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let chat_sent_recent = v["chat_sent_recent"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Some(ClientUiSnapshot {
        screen,
        creative_tab_id: v["creative_tab_id"].as_str().map(str::to_owned),
        creative_selected: v["creative_selected"].as_u64().unwrap_or(0) as usize,
        creative_page: v["creative_page"].as_u64().unwrap_or(0) as u32,
        creative_scroll: v["creative_scroll"].as_f64().unwrap_or(0.0) as f32,
        creative_search: v["creative_search"].as_str().unwrap_or("").to_owned(),
        creative_search_cursor: v["creative_search_cursor"].as_u64().unwrap_or(0) as usize,
        creative_search_highlight: v["creative_search_highlight"].as_u64().unwrap_or(0) as usize,
        chat_input: v["chat_input"].as_str().unwrap_or("").to_owned(),
        chat_cursor: v["chat_cursor"].as_u64().unwrap_or(0) as usize,
        chat_highlight: v["chat_highlight"].as_u64().unwrap_or(0) as usize,
        chat_display: v["chat_display"].as_u64().unwrap_or(0) as usize,
        chat_draft,
        chat_draft_shown: v["chat_draft_shown"].as_bool().unwrap_or(false),
        in_bed_chat: v["in_bed_chat"].as_bool().unwrap_or(false),
        has_chat_log,
        chat_log,
        chat_sent_recent,
        carried: ui_decode_item(&v["carried"]),
        craft_grid: ui_decode_items(&v["craft_grid"]),
        craft_result: ui_decode_item(&v["craft_result"]),
        craft_size: v["craft_size"].as_u64().unwrap_or(2) as usize,
        craft_table: ui_decode_pos3(&v["craft_table"]),
        container: ui_decode_pos3(&v["container"]),
        open_container_slots,
        ender_open: ui_decode_pos3(&v["ender_open"]),
        station: ui_station_from_json(&v["station"]),
        recipe_ui,
        recipe_book_open,
        recipe_book_filtering,
        mount_menu,
        sign_edit,
        book,
        command_block,
        gamemode_switcher_selected: v["gamemode_switcher_selected"].as_i64().map(|x| x as i32),
        perspective: v["perspective"].as_u64().map(|x| (x as u8).min(2)).unwrap_or(0),
    })
}

fn ui_slot_file(world_root: &Path, slot: &str) -> PathBuf {
    slot_dir(world_root, slot).join("rreturn_ui.json")
}

pub fn save_client_ui_slot(world_root: &Path, slot: &str, snap: ClientUiSnapshot) {
    let resolved = resolve_ui_world_root(world_root);
    if !resolved.as_os_str().is_empty() {
        let dir = slot_dir(&resolved, slot);
        let path = ui_slot_file(&resolved, slot);
        let tmp = dir.join("rreturn_ui.json.tmp");
        let json = ui_snapshot_to_json(&snap);
        if fs::create_dir_all(&dir).is_ok()
            && let Ok(bytes) = serde_json::to_vec_pretty(&json)
            && fs::write(&tmp, bytes).is_ok()
        {
            let _ = fs::rename(&tmp, &path);
        }
    }
    CLIENT_UI_SLOTS.with(|c| {
        c.borrow_mut().insert(slot.to_owned(), snap);
    });
}

pub fn load_client_ui_slot(world_root: &Path, slot: &str) -> Option<ClientUiSnapshot> {
    if let Some(cached) = CLIENT_UI_SLOTS.with(|c| c.borrow().get(slot).cloned()) {
        return Some(cached);
    }
    let resolved = resolve_ui_world_root(world_root);
    if resolved.as_os_str().is_empty() {
        return None;
    }
    let raw = fs::read_to_string(ui_slot_file(&resolved, slot)).ok()?;
    let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let parsed = ui_snapshot_from_json(&val)?;
    CLIENT_UI_SLOTS.with(|c| {
        c.borrow_mut().insert(slot.to_owned(), parsed.clone());
    });
    Some(parsed)
}

fn ui_find_local_guest(srv: &Game<'_>) -> Option<(usize, u32)> {
    let host = srv.lan.as_ref()?;
    let target = srv
        .serving
        .as_ref()
        .map(|s| s.owner)
        .or_else(|| host.owner.as_ref().map(|o| o.uuid));
    let gi = target
        .and_then(|u| host.hub.guests.iter().position(|g| g.uuid == u))
        .or_else(|| (!host.hub.guests.is_empty()).then_some(0))?;
    Some((gi, host.hub.guests[gi].conn))
}

fn has_server_menu_ui(ui: &ClientUiSnapshot) -> bool {
    ui.station.is_some()
        || ui.craft_table.is_some()
        || ui.container.is_some()
        || ui.ender_open.is_some()
        || ui.mount_menu.is_some()
        || matches!(
            &ui.book,
            Some(BookScreenSnapshot::View {
                lectern: Some(_),
                ..
            })
        )
        || (ui.screen == "command_block" && ui.command_block.is_some())
}

fn reopen_guest_container_on_server(
    srv: &mut Game<'_>,
    ui: &ClientUiSnapshot,
    restore_items: bool,
) -> Option<i32> {
    let Some((gi, conn)) = ui_find_local_guest(srv) else {
        return None;
    };
    let guest_mode = srv.lan.as_ref().map(|host| host.hub.guests[gi].game_mode);
    let opening_menu = has_server_menu_ui(ui);

    // Every server menu opener runs `guest_close_all(conn)`, which returns
    // `g.carried` and `g.craft_grid` into `g.inventory`. Clear them temporarily
    // before opening the target menu so rollback does not duplicate held items
    // into hotbar slots, then restore and sync them afterward.
    let saved_held = if opening_menu {
        srv.lan.as_mut().map(|host| {
            host.hub.guests.update(gi, |g| {
                let carried = g.carried.take();
                let craft_grid = g.craft_grid.clone();
                let craft_result = g.craft_result.take();
                g.craft_grid.fill(None);
                (carried, craft_grid, craft_result)
            })
        })
    } else {
        None
    };

    if let Some(st) = &ui.station {
        let mut st_open = st.clone();
        if !restore_items {
            for slot in &mut st_open.inputs {
                *slot = None;
            }
            st_open.result = None;
        }
        let _ = srv.rreturn_lan_reopen_station(conn, &st_open);
    } else if let Some(pos) = ui.craft_table {
        let mut grid = if restore_items {
            ui.craft_grid.clone()
        } else {
            vec![None; 9]
        };
        grid.resize(9, None);
        let _ = srv.lan_open_crafting_with(conn, pos, grid);
    } else if let Some(pos) = ui.container.or(ui.ender_open) {
        let _ = srv.lan_open_container(conn, (pos[0], pos[1], pos[2]));
    } else if let Some(mm) = &ui.mount_menu {
        srv.lan_open_mount(conn, mm.mob);
    } else if let Some(BookScreenSnapshot::View {
        lectern: Some(pos),
        ..
    }) = &ui.book
    {
        srv.lan_open_lectern(conn, (pos[0], pos[1], pos[2]));
    } else if ui.screen == "command_block"
        && let Some(cb) = &ui.command_block
    {
        srv.lan_open_command_block(conn, (cb.pos[0], cb.pos[1], cb.pos[2]));
    }

    if let Some(host) = srv.lan.as_mut() {
        if restore_items {
            let _ = host.hub.guests.update(gi, |g| {
                g.carried = ui.carried.clone();
                if ui.craft_table.is_none() && ui.station.is_none() {
                    for i in 0..4 {
                        g.craft_grid[i] = ui.craft_grid.get(i).cloned().flatten();
                    }
                }
            });
            if opening_menu {
                let cursor = host.hub.stack(&ui.carried);
                host.hub.guests[gi].send(&C::SetCursorItem(play::SetCursorItem {
                    item: cursor,
                }));
            } else {
                host.hub.send_inventory(gi);
            }
        } else if let Some((prev_carried, prev_craft_grid, prev_craft_result)) = saved_held {
            let cursor = host.hub.stack(&prev_carried);
            let _ = host.hub.guests.update(gi, |g| {
                g.carried = prev_carried;
                g.craft_grid = prev_craft_grid;
                g.craft_result = prev_craft_result;
            });
            host.hub.guests[gi].send(&C::SetCursorItem(play::SetCursorItem {
                item: cursor,
            }));
        }
        if ui.container.is_some() {
            host.hub.sync_containers(&srv.world);
        }
    }
    guest_mode
}

pub fn apply_client_ui_exact(
    game: &mut Game<'_>,
    ui: &ClientUiSnapshot,
    restore_items: bool,
    restore_recipe_book: bool,
    is_scrub_preview: bool,
) {
    apply_chat_history_from_snapshot(game, ui, !is_scrub_preview);
    game.ui.chat_draft = ui.chat_draft.as_ref().map(|(text, command)| ChatDraft {
        text: text.clone(),
        command: *command,
    });
    game.ui.chat_draft_shown = ui.chat_draft_shown;
    game.ui.in_bed_chat = ui.in_bed_chat;

    if ui.screen != "chat" {
        game.ui.chat_screen = None;
    }
    if ui.sign_edit.is_none() {
        game.rreturn_restore_sign_edit(None);
    }
    if ui.book.is_none() {
        game.rreturn_restore_book(None);
    }
    if ui.screen != "command_block" {
        game.rreturn_restore_command_block_ui(None);
    }
    if ui.screen != "game_mode_switcher" {
        game.gamemode_switcher = None;
    }
    game.rreturn_set_camera_type(ui.perspective);

    match ui.screen.as_str() {
        "creative" => {
            close_client_container_local(game);
            game.refresh_creative_tabs();
            game.refresh_saved_hotbars();
            if !game.creative_screen.tabs.is_empty() {
                game.creative_screen.set_page(ui.creative_page);
                let idx = ui
                    .creative_tab_id
                    .as_ref()
                    .and_then(|id| game.creative_screen.tabs.iter().position(|t| &t.id == id))
                    .unwrap_or_else(|| {
                        ui.creative_selected
                            .min(game.creative_screen.tabs.len().saturating_sub(1))
                    });
                if game.creative_screen.selected != idx {
                    game.creative_screen.select(idx);
                }
                game.creative_screen.page = ui
                    .creative_page
                    .min(game.creative_screen.pages().saturating_sub(1));
            }
            game.creative_screen.scroll = ui.creative_scroll;
            game.creative_screen.search.set_value(&ui.creative_search);
            let max_c = ui.creative_search.chars().count();
            game.creative_screen.search.edit.cursor = ui.creative_search_cursor.min(max_c);
            game.creative_screen.search.edit.highlight = ui.creative_search_highlight.min(max_c);
            game.set_screen(Screen::Creative);
            game.build_screen();
            if !is_scrub_preview {
                game.capture(false);
            }
        }
        "inventory" => {
            if is_scrub_preview
                && let Some(r) = game.remote.as_mut()
                && (ui.station.is_some()
                    || ui.container.is_some()
                    || ui.ender_open.is_some()
                    || ui.craft_table.is_some()
                    || ui.craft_size >= 3
                    || ui.mount_menu.is_some())
            {
                r.rreturn_clear_remote_menu();
            }
            if let Some(st) = &ui.station {
                let mut st_apply = st.clone();
                if !restore_items {
                    for s in &mut st_apply.inputs {
                        *s = None;
                    }
                    st_apply.result = None;
                }
                game.container = None;
                game.open_container_slots = None;
                game.ender_open = None;
                game.craft_table = None;
                game.rreturn_apply_mount_client_ui(None);
                game.rreturn_apply_station_client_ui(&st_apply);
            } else if let Some(mm) = &ui.mount_menu {
                let mut mm_apply = mm.clone();
                if !restore_items {
                    mm_apply.saddle = None;
                    mm_apply.armor = None;
                    for s in &mut mm_apply.items {
                        *s = None;
                    }
                }
                game.station = None;
                game.container = None;
                game.open_container_slots = None;
                game.ender_open = None;
                game.craft_table = None;
                game.rreturn_apply_mount_client_ui(Some(&mm_apply));
            } else if ui.container.is_some()
                || ui.ender_open.is_some()
                || ui.craft_table.is_some()
                || ui.craft_size >= 3
            {
                game.station = None;
                game.rreturn_apply_mount_client_ui(None);
                if ui.container.is_some() || ui.ender_open.is_some() {
                    if ui.container.is_some() || game.container.is_none() || is_scrub_preview {
                        game.container = ui.container.map(|p| (p[0], p[1], p[2]));
                    }
                    if let Some((p, n)) = ui.open_container_slots {
                        game.open_container_slots = Some(((p[0], p[1], p[2]), n));
                    }
                    game.ender_open = ui.ender_open.map(|p| (p[0], p[1], p[2]));
                    game.craft_table = None;
                    game.craft_size = ui.craft_size.max(2);
                } else {
                    game.container = None;
                    game.open_container_slots = None;
                    game.ender_open = None;
                    game.craft_table = if game.remote.is_none() || is_scrub_preview {
                        ui.craft_table
                    } else {
                        None
                    };
                    game.craft_size = ui.craft_size.max(3);
                }
            } else {
                close_client_container_local(game);
                game.station = None;
                game.container = None;
                game.open_container_slots = None;
                game.ender_open = None;
                game.craft_table = None;
                game.rreturn_apply_mount_client_ui(None);
                game.craft_size = 2;
            }
            if restore_recipe_book {
                game.level.recipe_book.open = ui.recipe_book_open;
                game.level.recipe_book.filtering = ui.recipe_book_filtering;
                game.rreturn_restore_recipe_ui(&ui.recipe_ui);
            }
            game.set_screen(Screen::Inventory);
            game.build_screen();
            if !is_scrub_preview {
                game.capture(false);
            }
        }
        "chat" => {
            close_client_container_local(game);
            let preserve_draft = ui.chat_draft.as_ref().map(|(t, c)| ChatDraft {
                text: t.clone(),
                command: *c,
            });
            let preserve_shown = ui.chat_draft_shown;
            if game.screen != Screen::Chat || game.ui.chat_screen.is_none() {
                game.begin_chat_screen(&ui.chat_input, false);
            }
            if let Some(cs) = game.ui.chat_screen.as_mut() {
                let max_c = ui.chat_input.chars().count();
                cs.input.value = ui.chat_input.clone();
                cs.input.cursor = ui.chat_cursor.min(max_c);
                cs.input.highlight = ui.chat_highlight.min(max_c);
                cs.input.display = ui.chat_display.min(max_c);
                cs.input.focused = true;
            }
            game.ui.chat_draft = preserve_draft;
            game.ui.chat_draft_shown = preserve_shown;
            game.set_screen(Screen::Chat);
            if !is_scrub_preview {
                game.capture(false);
            }
        }
        "command_block" => {
            close_client_container_local(game);
            game.rreturn_restore_command_block_ui(ui.command_block.as_ref());
            if !is_scrub_preview {
                game.capture(false);
            }
        }
        "playing" => {
            if game.screen != Screen::Playing {
                close_client_container_local(game);
                game.set_screen(Screen::Playing);
            } else if ui.sign_edit.is_none() && ui.book.is_none() {
                close_client_container_local(game);
            }
            if let Some(se) = &ui.sign_edit {
                game.rreturn_restore_sign_edit(Some(se));
            } else if let Some(bk) = &ui.book {
                game.rreturn_restore_book(Some(bk));
            } else if !is_scrub_preview {
                game.capture(true);
            }
        }
        "game_mode_switcher" => {
            close_client_container_local(game);
            let cur_mode = mode_from_id(game.mode_id());
            let selected_mode = ui.gamemode_switcher_selected.map(mode_from_id);
            let mut switcher = GameModeSwitcher::new(cur_mode, game.previous_mode);
            if let Some(sel) = selected_mode {
                switcher.selected = sel;
            }
            game.gamemode_switcher = Some(switcher);
            game.set_screen(Screen::GameModeSwitcher);
            if !is_scrub_preview {
                game.capture(false);
            }
        }
        _ => {
            close_client_container_local(game);
            game.set_screen(Screen::Playing);
            if !is_scrub_preview {
                game.capture(true);
            }
        }
    }

    if restore_items {
        game.carried = ui.carried.clone();
        if ui.craft_table.is_some() || ui.craft_size >= 3 || ui.station.is_none() {
            let need = ui.craft_grid.len().max(game.craft_size * game.craft_size);
            if game.craft_grid.len() < need {
                game.craft_grid.resize(need, None);
            }
            for (i, slot) in game.craft_grid.iter_mut().enumerate() {
                *slot = ui.craft_grid.get(i).cloned().flatten();
            }
            if ui.screen == "inventory"
                && (is_scrub_preview
                    || (game.rreturn_craft_result().is_none() && ui.craft_result.is_some()))
            {
                game.rreturn_set_craft_result(ui.craft_result.clone());
            }
        }
    }
}

pub fn restore_client_ui_on_rollback(
    game: &mut Game<'_>,
    ui: &ClientUiSnapshot,
    policy: &RollbackPolicy,
) {
    arm_ui_mode_guard(6);
    let _ = game.poll_remote();
    let restore_items = policy.player_inventory;
    let restore_recipe_book = policy.player_recipe_book;
    let has_server_menu = has_server_menu_ui(ui);

    let mut srv_game_mode = None;
    if let Some(server) = &game.integrated
        && (has_server_menu || restore_items || policy.player_abilities)
    {
        let remote_target = ui
            .container
            .or(ui.ender_open)
            .or(ui.craft_table)
            .or(ui.station.as_ref().map(|s| s.pos));
        if let Some(r) = game.remote.as_mut() {
            r.rreturn_set_use_target(remote_target);
        }
        let ui_srv = ui.clone();
        srv_game_mode = server
            .call(move |srv| reopen_guest_container_on_server(srv, &ui_srv, restore_items))
            .flatten();
        let _ = game.poll_remote();
    }
    if policy.player_abilities
        && let Some(gm) = srv_game_mode
        && gm != game.mode_id()
    {
        game.set_mode_id(gm);
    }
    apply_client_ui_exact(game, ui, restore_items, restore_recipe_book, false);
    if restore_items {
        game.rreturn_sync_hands_to_inventory();
        game.rreturn_sync_creative_cache();
    }
}

pub fn ui_state_json(game: &Game<'_>) -> serde_json::Value {
    let s = capture_client_ui_snapshot(game);
    let mut creative_tabs = serde_json::Map::new();
    let mut creative_slot0 = serde_json::Value::Null;
    if game.screen == Screen::Creative {
        let (sw, sh) = game.gui_size();
        let ox = ((sw - horizon_client::creative::Creative::WIDTH) * 0.5).trunc();
        let oy = ((sh - horizon_client::creative::Creative::HEIGHT) * 0.5).trunc();
        for t in &game.creative_screen.tabs {
            if game.creative_screen.on_page(t) {
                let (tx, ty) = horizon_client::creative::Creative::tab_position(t);
                creative_tabs.insert(
                    t.id.clone(),
                    serde_json::json!([
                        (ox + tx + 13.0).round() as i32,
                        (oy + ty + 14.0).round() as i32
                    ]),
                );
            }
        }
        creative_slot0 = serde_json::json!([
            (ox + 9.0 + 8.0).round() as i32,
            (oy + 18.0 + 8.0).round() as i32
        ]);
    }
    let mut out = serde_json::json!({
        "screen": s.screen,
        "creative_tab": s.creative_tab_id,
        "creative_selected": s.creative_selected,
        "creative_page": s.creative_page,
        "creative_scroll": s.creative_scroll,
        "creative_search": s.creative_search,
        "creative_search_cursor": s.creative_search_cursor,
        "creative_tabs": creative_tabs,
        "creative_slot0": creative_slot0,
        "chat_input": s.chat_input,
        "chat_cursor": s.chat_cursor,
        "chat_draft": s.chat_draft.as_ref().map(|(t, c)| serde_json::json!([t, c])),
        "chat_log": s.chat_log.iter().map(|e| Text::from(e.component.clone()).plain()).collect::<Vec<_>>(),
        "chat_recent": s.chat_sent_recent,
        "carried": s.carried.as_ref().map(|st| st.item.to_string()),
        "carried_count": s.carried.as_ref().map(|st| st.count).unwrap_or(0),
        "craft_size": s.craft_size,
        "craft_grid_items": s.craft_grid.iter().map(|it| it.as_ref().map(|st| serde_json::json!([st.item.to_string(), st.count]))).collect::<Vec<_>>(),
        "craft_result": s.craft_result.as_ref().map(|st| serde_json::json!([st.item.to_string(), st.count])),
        "craft_table": s.craft_table,
        "container": s.container,
        "container_be": game.container.and_then(|pos| game.world.block_entities.get(&pos)).map(|be| match be {
            BlockEntity::Furnace(f) => serde_json::json!({
                "kind": "furnace",
                "lit_remaining": f.lit_remaining,
                "lit_total": f.lit_total,
                "cooking_timer": f.cooking_timer,
                "cooking_total": f.cooking_total,
                "items": f.items.iter().map(|it| it.as_ref().map(|st| serde_json::json!([st.item.to_string(), st.count]))).collect::<Vec<_>>(),
            }),
            BlockEntity::Brewing(b) => serde_json::json!({
                "kind": "brewing",
                "brew_time": b.brew_time,
                "total_brew_time": b.total_brew_time,
                "fuel": b.fuel,
                "total_fuel": b.total_fuel,
            }),
            BlockEntity::Crafter(c) => serde_json::json!({
                "kind": "crafter",
                "disabled": c.disabled,
                "triggered": c.triggered,
                "crafting_ticks": c.crafting_ticks,
            }),
            BlockEntity::Chest(items) => serde_json::json!({
                "kind": "chest",
                "count": items.iter().filter(|x| x.is_some()).count(),
            }),
            _ => serde_json::json!({ "kind": "other" }),
        }),
        "ender_open": s.ender_open,
        "gamemode_switcher_selected": s.gamemode_switcher_selected,
        "client_game_mode": game.mode_id(),
        "creative": game.creative,
        "xp_level": game.experience.level,
        "xp_progress": game.experience.progress,
        "station_kind": s.station.as_ref().map(|st| st.kind.clone()),
        "anvil_name": s.station.as_ref().map(|st| st.anvil_name.clone()),
        "station_inputs": s.station.as_ref().map(|st| st.inputs.iter().map(|it| it.as_ref().map(|i| i.item.to_string())).collect::<Vec<_>>()),
        "sign_edit": s.sign_edit.as_ref().map(|se| serde_json::json!({
            "pos": se.pos,
            "line": se.line,
            "lines": se.lines,
            "edit_value": se.edit_value,
            "edit_cursor": se.edit_cursor,
        })),
        "book": s.book.as_ref().map(|bk| match bk {
            BookScreenSnapshot::Edit { page, page_value, title_value, signing, .. } => serde_json::json!({
                "mode": "edit", "page": page, "page_value": page_value, "title_value": title_value, "signing": signing
            }),
            BookScreenSnapshot::View { page, lectern, .. } => serde_json::json!({
                "mode": "view", "page": page, "lectern": lectern
            }),
        }),
        "command_block": s.command_block.as_ref().map(|cb| serde_json::json!({
            "pos": cb.pos, "command": cb.command, "cursor": cb.cursor
        })),
        "text_input_focused": is_text_input_focused(game),
        "pointer_captured": game.rreturn_pointer_captured(),
        "scrubbing": is_scrubbing(),
    });
    if let Some(map) = out.as_object_mut() {
        let (vis_hands, _) = game.rreturn_hand_visual_state();
        map.insert("perspective".into(), serde_json::json!(s.perspective));
        map.insert(
            "perspective_name".into(),
            serde_json::json!(match s.perspective {
                1 => "third_person_back",
                2 => "third_person_front",
                _ => "first_person",
            }),
        );
        map.insert("cursor".into(), serde_json::json!([game.cursor.0, game.cursor.1]));
        map.insert("virtual_cursor".into(), serde_json::json!(get_scrub_virtual_cursor()));
        map.insert(
            "visible_main_hand".into(),
            serde_json::json!(vis_hands[0].1.as_ref().map(|st| st.item.to_string())),
        );
        map.insert(
            "visible_off_hand".into(),
            serde_json::json!(vis_hands[1].1.as_ref().map(|st| st.item.to_string())),
        );
    }
    out
}
