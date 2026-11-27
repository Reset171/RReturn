"""Tests 112-114: Multi-screen hotkeys, exact UI/cursor/chat restore, and reversible UI scrubbing."""

import re
import tempfile
from pathlib import Path
from . import setup_clean_env

SCRATCH_DIR = Path(tempfile.gettempdir()) / "rreturn_e2e_screenshots"


def _ui(h):
    st = h.rreturn("ui state")
    assert isinstance(st, dict), f"Expected JSON from 'rreturn ui state', got: {st}"
    return st


def _tick(h):
    st = h.rreturn("status --json")
    assert isinstance(st, dict) and isinstance(st.get("game_time"), int), f"Invalid status JSON: {st}"
    return st["game_time"]


def _scrub_seek_to_tick(h, target_tick):
    info = h.rreturn("scrub seek_ticks 0")
    m = re.search(r"to tick\s+(-?\d+(?:\.\d+)?)\s+\(", str(info))
    assert m is not None, f"Unexpected scrub seek output: {info}"
    curr_tick = float(m.group(1))
    delta = round(float(target_tick) - curr_tick)
    res = h.rreturn(f"scrub seek_ticks {delta}")
    assert re.search(r"to tick\s+(-?\d+(?:\.\d+)?)\s+\(", str(res)) is not None, (
        f"Failed scrub seek to {target_tick} (delta={delta}): {res}"
    )


def test_112_scrub_blocks_chat_and_hud_controls(h):
    """T and / must never open Chat during active scrub; Enter commits, Esc cancels, dark HUD renders."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.rreturn("scrub on")
    h.wait(25)

    try:
        # До скраба на игровом экране курсор захвачен
        assert _ui(h)["pointer_captured"] is True, f"Expected captured pointer before scrub: {_ui(h)}"
        assert h.state().get("pointer_grabbed") is True, h.state()

        # 1. Start scrub with U key — курсор должен освободиться для HUD скраба
        h.raw_send("key u tap")
        h.wait(2)
        ui = _ui(h)
        assert ui["scrubbing"] is True, f"Scrub did not start on U: {ui}"
        assert ui["pointer_captured"] is False, f"Cursor remained captured on Playing scrub start: {ui}"
        assert h.state().get("pointer_grabbed") is False, h.state()

        # 2. Press T and / while scrubbing — chat must remain blocked!
        h.raw_send("key t tap")
        h.wait(2)
        h.raw_send("key slash tap")
        h.wait(2)
        assert h.screen().get("screen") == "playing", f"T or / opened chat during scrub: {h.screen()}"
        ui_mid = _ui(h)
        assert ui_mid["screen"] == "playing" and ui_mid["scrubbing"] is True, ui_mid

        # 3. Step back via A/Left and capture clean screenshot of redesigned dark floating HUD
        h.raw_send("key f3 down")
        h.raw_send("key d tap")
        h.raw_send("key f3 up")
        h.wait(1)
        h.raw_send("key a tap")
        h.wait(2)
        SCRATCH_DIR.mkdir(parents=True, exist_ok=True)
        h.raw_send(f"screenshot {SCRATCH_DIR / 'scrub_playing.png'}")
        h.wait(1)

        # Verify Apply/Cancel capsule bounds are reported cleanly
        hud_bounds = h.rreturn("ui scrub_hud")
        assert isinstance(hud_bounds, dict) and hud_bounds.get("capsule", [0, 0, 0, 0])[2] > 200, (
            f"Scrub HUD bounds invalid: {hud_bounds}"
        )

        # 4. Enter commits scrub cleanly and restores pointer capture on Playing
        h.raw_send("key enter tap")
        h.wait(3)
        ui_commit = _ui(h)
        assert ui_commit["scrubbing"] is False, f"Enter failed to commit scrub: {ui_commit}"
        assert h.screen().get("screen") == "playing"
        assert ui_commit["pointer_captured"] is True, f"Cursor stayed uncaptured after Playing commit: {ui_commit}"

        # 5. Re-enter scrub (pointer frees) and Esc cancels cleanly (pointer restores)
        h.wait(8)
        h.raw_send("key u tap")
        h.wait(2)
        assert _ui(h)["scrubbing"] is True
        assert _ui(h)["pointer_captured"] is False
        h.raw_send("key left tap")
        h.wait(2)
        h.raw_send("key esc tap")
        h.wait(3)
        ui_cancel = _ui(h)
        assert ui_cancel["scrubbing"] is False, f"Esc failed to cancel scrub: {ui_cancel}"
        assert h.screen().get("screen") == "playing"
        assert ui_cancel["pointer_captured"] is True, f"Cursor stayed uncaptured after Playing cancel: {ui_cancel}"
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.wait(2)


def test_113_checkpoint_inside_creative_chat_and_containers(h):
    """F6/F7 inside Creative, Chat, and Chest restores exact tab, cursor item, unsent chat text, and container even across purge_memory."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)

    try:
        # ---- Part A: Creative Colored Blocks tab + Cursor Item + F6/F7 + disk purge_memory ----
        h.command("gamemode creative")
        h.wait(2)
        h.raw_send("key e tap")
        h.wait(3)
        ui_c = _ui(h)
        assert ui_c["screen"] == "creative", f"Expected creative screen: {ui_c}"
        cb_pos = ui_c["creative_tabs"].get("minecraft:colored_blocks")
        assert cb_pos, f"Colored blocks tab not exposed in creative_tabs: {ui_c['creative_tabs']}"
        h.raw_send(f"click-at {cb_pos[0]} {cb_pos[1]}")
        h.wait(2)
        ui_tab = _ui(h)
        assert ui_tab["creative_tab"] == "minecraft:colored_blocks", f"Tab didn't switch: {ui_tab}"

        # Pick item from first colored_blocks display slot onto cursor
        slot0 = ui_tab["creative_slot0"]
        h.raw_send(f"click-at {slot0[0]} {slot0[1]}")
        h.wait(2)
        ui_cur = _ui(h)
        saved_carried = ui_cur["carried"]
        assert saved_carried is not None and ui_cur["carried_count"] >= 1, (
            f"Expected colored block carried on cursor: {ui_cur}"
        )

        # Save checkpoint using F6 DIRECTLY inside Creative screen
        h.raw_send("key f6 tap")
        h.wait(3)

        # Drop/close screen, change gamemode, wipe inventory & purge RAM snapshot cache
        h.raw_send("key esc tap")
        h.wait(2)
        h.command("gamemode survival")
        h.command("clear @s")
        h.wait(2)
        h.rreturn("purge_memory")
        h.wait(2)

        # Rollback via F7
        h.raw_send("key key.keyboard.f7 tap")
        h.wait(5)
        ui_restored = _ui(h)
        assert ui_restored["screen"] == "creative", f"Expected return to creative: {ui_restored}"
        assert ui_restored["creative_tab"] == "minecraft:colored_blocks", (
            f"Expected minecraft:colored_blocks tab after rollback: {ui_restored}"
        )
        assert ui_restored["carried"] == saved_carried, (
            f"Expected carried cursor item {saved_carried}, got: {ui_restored}"
        )

        h.raw_send("key esc tap")
        h.wait(2)
        h.command("gamemode survival")
        h.command("clear @s")
        h.wait(2)

        # ---- Part B: Unsent Chat text + cursor position restore across F6 -> Inventory -> purge_memory -> F7 ----
        h.raw_send("key t tap")
        h.wait(2)
        assert _ui(h)["screen"] == "chat"
        draft_text = "hello_unsent_chat"
        h.raw_send(f"type {draft_text}")
        for _ in range(5):
            h.raw_send("key left tap")
        h.wait(2)
        ui_chat = _ui(h)
        expected_cursor = len(draft_text) - 5
        assert ui_chat["chat_input"] == draft_text and ui_chat["chat_cursor"] == expected_cursor, (
            f"Chat pre-save mismatch: {ui_chat}"
        )

        # Press F6 directly while inside Chat!
        h.raw_send("key f6 tap")
        h.wait(3)

        # Exit chat, open Inventory, purge memory, press F7 directly inside Inventory
        h.raw_send("key esc tap")
        h.wait(2)
        h.raw_send("key e tap")
        h.wait(2)
        assert _ui(h)["screen"] == "inventory"
        h.rreturn("purge_memory")
        h.wait(2)

        h.raw_send("key key.keyboard.f7 tap")
        h.wait(5)
        ui_chat_restored = _ui(h)
        assert ui_chat_restored["screen"] == "chat", f"Did not restore chat screen: {ui_chat_restored}"
        assert ui_chat_restored["chat_input"] == draft_text, (
            f"Did not restore unsent chat text: {ui_chat_restored}"
        )
        assert ui_chat_restored["chat_cursor"] == expected_cursor, (
            f"Did not restore chat cursor {expected_cursor}: {ui_chat_restored}"
        )
        h.raw_send("key esc tap")
        h.wait(2)

        # ---- Part C: Chest container + carried cursor restore across disk reload ----
        h.command("setblock 0 65 2 minecraft:chest")
        h.command('data modify block 0 65 2 Items set value [{Slot:0b,id:"minecraft:diamond",count:5}]')
        h.wait(3)
        h.interact(0, 65, 2)
        h.wait(3)
        ui_box = _ui(h)
        assert ui_box["container"] == [0, 65, 2], f"Chest did not open: {ui_box}"

        h.click_slot(300)
        h.wait(2)
        assert _ui(h)["carried"] == "minecraft:diamond", f"Failed to pick diamond onto cursor: {_ui(h)}"

        h.raw_send("key f6 tap")
        h.wait(3)
        h.raw_send("key esc tap")
        h.wait(2)
        h.command("setblock 0 65 2 minecraft:air")
        h.command("clear @s")
        h.rreturn("purge_memory")
        h.wait(2)

        h.raw_send("key key.keyboard.f7 tap")
        h.wait(5)
        ui_box_restored = _ui(h)
        assert ui_box_restored["screen"] == "inventory" and ui_box_restored["container"] == [0, 65, 2], (
            f"Chest UI not restored on rollback: {ui_box_restored}"
        )
        assert ui_box_restored["carried"] == "minecraft:diamond", (
            f"Cursor diamond in chest not restored: {ui_box_restored}"
        )
        # Put cursor item back into chest slot 300 to confirm server menu handle is genuinely synced
        h.click_slot(300)
        h.wait(2)
        assert _ui(h)["carried"] is None
        h.raw_send("key esc tap")
        h.wait(2)
        block_items = h.data_get_block(0, 65, 2, "Items")
        assert "minecraft:diamond" in str(block_items), f"Server chest did not accept returned cursor item: {block_items}"
    finally:
        h.close_screen()
        h.command("setblock 0 65 2 minecraft:air")
        h.command("gamemode survival")
        h.command("clear @s")
        h.wait(2)


def test_114_text_focus_ctrl_u_and_reversible_ui_scrub(h):
    """Bare U types in text fields while Ctrl+U triggers scrub; Creative/Inventory/Chat/Cursor states scrub reversibly."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.rreturn("scrub on")
    h.wait(10)

    try:
        # 1. Text box focused (Chat): bare U types 'u', Ctrl+U starts scrub!
        h.raw_send("key t tap")
        h.wait(2)
        h.raw_send("type test")
        h.raw_send("key u tap")
        h.raw_send("type u")
        h.wait(2)
        ui_type = _ui(h)
        assert ui_type["scrubbing"] is False, f"Bare U incorrectly triggered scrub while typing: {ui_type}"
        assert ui_type["chat_input"] == "testu", f"Bare U was swallowed from Chat input: {ui_type}"

        h.raw_send("key ctrl down")
        h.raw_send("key u tap")
        h.raw_send("key ctrl up")
        h.wait(2)
        assert _ui(h)["scrubbing"] is True, "Ctrl+U failed to trigger scrub inside focused Chat"
        h.raw_send("key esc tap")
        h.wait(2)
        assert _ui(h)["scrubbing"] is False
        h.raw_send("key esc tap")
        h.wait(2)

        # 2. Reversible Creative tab & cursor scrub + HUD screenshot + Apply button click
        h.command("gamemode creative")
        h.wait(2)
        h.raw_send("key e tap")
        h.wait(3)
        ui_1 = _ui(h)
        cb_pos = ui_1["creative_tabs"]["minecraft:colored_blocks"]
        h.raw_send(f"click-at {cb_pos[0]} {cb_pos[1]}")
        h.wait(2)
        slot0 = _ui(h)["creative_slot0"]
        h.raw_send(f"click-at {slot0[0]} {slot0[1]}")
        h.wait(2)
        item_t1 = _ui(h)["carried"]
        assert item_t1 is not None, f"Failed to pick item onto cursor at T1: {_ui(h)}"

        # Hold at T1 (~20 ticks) so colored_blocks + carried cursor is firmly inside buffer
        h.wait(20)

        # Now clear cursor & move to building_blocks tab for T2
        h.raw_send("key esc tap")
        h.wait(2)
        h.command("clear @s")
        h.raw_send("key e tap")
        h.wait(2)
        ui_2 = _ui(h)
        bb_pos = ui_2["creative_tabs"].get("minecraft:building_blocks")
        if bb_pos:
            h.raw_send(f"click-at {bb_pos[0]} {bb_pos[1]}")
        h.wait(10)
        assert _ui(h)["carried"] is None

        # Trigger scrubbing with bare U directly inside Creative (non-search tab!)
        h.raw_send("key u tap")
        h.wait(2)
        assert _ui(h)["scrubbing"] is True, "Bare U did not trigger scrub inside non-text Creative tab"

        # Step back to T1 while keeping interactive scrub active over Creative screen
        h.rreturn("scrub seek -1.6")
        h.wait(2)
        SCRATCH_DIR.mkdir(parents=True, exist_ok=True)
        h.raw_send(f"screenshot {SCRATCH_DIR / 'scrub_creative.png'}")
        h.wait(1)

        ui_preview = _ui(h)
        assert ui_preview["creative_tab"] == "minecraft:colored_blocks", (
            f"Scrub preview did not rewind to colored_blocks: {ui_preview}"
        )
        assert ui_preview["carried"] == item_t1, (
            f"Scrub preview did not rewind cursor item to {item_t1}: {ui_preview}"
        )

        # Click the floating HUD Apply pill directly!
        hud_bounds = h.rreturn("ui scrub_hud")
        ax, ay = hud_bounds["apply_center"]
        h.raw_send(f"click-at {ax} {ay}")
        h.wait(4)

        ui_final = _ui(h)
        assert ui_final["scrubbing"] is False, f"HUD Apply pill did not commit scrub: {ui_final}"
        assert ui_final["screen"] == "creative", f"Expected committed screen creative: {ui_final}"
        assert ui_final["creative_tab"] == "minecraft:colored_blocks", ui_final
        assert ui_final["carried"] == item_t1, f"Committed cursor mismatch: {ui_final}"

        h.raw_send("key esc tap")
        h.wait(2)
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.close_screen()
        h.command("gamemode survival")
        h.command("clear @s")
        h.wait(2)


def test_119_crafting_table_scrub_and_post_rollback_interaction(h):
    """Crafting Table keeps 3x3 grid across scrub preview/commit/cancel, F6/F7, and post-scrub right-click."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("setblock 0 65 2 minecraft:crafting_table")
    h.command("setblock 1 65 2 minecraft:chest")
    h.wait(4)
    h.rreturn("scrub on")
    h.wait(8)

    try:
        # 1. Open crafting table -> record ticks -> close -> scrub back into open crafting table
        h.interact(0, 65, 2)
        h.wait(10)
        ui_open = _ui(h)
        assert ui_open["screen"] == "inventory", ui_open
        assert ui_open["craft_size"] == 3, f"Expected craft_size 3: {ui_open}"
        assert ui_open["craft_table"] == [0, 65, 2], f"Expected craft_table [0,65,2]: {ui_open}"
        h.raw_send("key esc tap")
        h.wait(10)
        assert _ui(h)["screen"] == "playing"

        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -15")
        h.wait(2)
        ui_prev = _ui(h)
        assert ui_prev["scrubbing"] is True
        assert ui_prev["pointer_captured"] is False, ui_prev
        assert ui_prev["screen"] == "inventory", f"Scrub preview closed craft table: {ui_prev}"
        assert ui_prev["craft_size"] == 3, f"Scrub collapsed 3x3 crafting table to 2x2: {ui_prev}"
        assert ui_prev["craft_table"] == [0, 65, 2], ui_prev
        scr_prev = h.screen()
        assert scr_prev.get("craft_size") == 3, scr_prev
        hud_craft = h.rreturn("ui scrub_hud")
        assert isinstance(hud_craft, dict) and hud_craft["capsule"][3] <= 24.0, (
            f"Expected compact 1-row scrub HUD when Inventory/Crafting is open: {hud_craft}"
        )

        # Scrub forward onto Playing while still scrubbing — pointer must stay uncaptured!
        h.rreturn("scrub seek_ticks 12")
        h.wait(2)
        ui_fwd_play = _ui(h)
        assert ui_fwd_play["screen"] == "playing" and ui_fwd_play["scrubbing"] is True, ui_fwd_play
        assert ui_fwd_play["pointer_captured"] is False, f"Re-grabbed cursor on Playing during scrub: {ui_fwd_play}"

        # Seek back to open crafting table, commit, verify craft_size=3 and cursor free
        h.rreturn("scrub seek_ticks -12")
        h.wait(2)
        h.raw_send("key enter tap")
        h.wait(3)
        ui_comm = _ui(h)
        assert ui_comm["scrubbing"] is False
        assert ui_comm["pointer_captured"] is False, ui_comm
        assert ui_comm["screen"] == "inventory" and ui_comm["craft_size"] == 3, ui_comm
        assert ui_comm["craft_table"] == [0, 65, 2], ui_comm

        # 2. Anchor while inside Crafting Table -> U -> seek -> Esc cancel must restore 3x3 Crafting Table
        h.raw_send("key u tap")
        h.wait(2)
        assert _ui(h)["craft_size"] == 3, f"Opening scrub collapsed 3x3 craft table at anchor: {_ui(h)}"
        assert _ui(h)["pointer_captured"] is False
        h.rreturn("scrub seek_ticks -12")
        h.wait(2)
        h.raw_send("key esc tap")
        h.wait(3)
        ui_canc = _ui(h)
        assert ui_canc["scrubbing"] is False
        assert ui_canc["pointer_captured"] is False, ui_canc
        assert ui_canc["screen"] == "inventory" and ui_canc["craft_size"] == 3, ui_canc
        h.raw_send("key esc tap")
        h.wait(3)
        assert _ui(h)["pointer_captured"] is True, _ui(h)

        # 3. Real look + right mouse click on Crafting Table after scrub MUST open Crafting Table (not 2x2 inventory)
        h.raw_send("look 0 25")
        h.wait(1)
        h.raw_send("mouse right click")
        h.wait(3)
        ui_post = _ui(h)
        scr_post = h.screen()
        assert ui_post["screen"] == "inventory", ui_post
        assert ui_post["craft_size"] == 3, f"Right-click crafting table opened 2x2 inventory instead of 3x3: {ui_post}"
        assert ui_post["craft_table"] == [0, 65, 2], ui_post
        assert scr_post.get("craft_size") == 3, scr_post

        # 4. Also verify F6 inside Crafting Table -> switch to Chest -> F7 restores Crafting Table cleanly
        h.rreturn("save ui_craft_slot")
        h.raw_send("key esc tap")
        h.wait(2)
        h.interact(1, 65, 2)
        h.wait(3)
        assert _ui(h)["container"] == [1, 65, 2]
        h.rreturn("load ui_craft_slot")
        h.wait(3)
        ui_loaded = _ui(h)
        assert ui_loaded["screen"] == "inventory" and ui_loaded["craft_size"] == 3, ui_loaded
        assert ui_loaded["craft_table"] == [0, 65, 2] and ui_loaded["container"] is None, ui_loaded
        h.raw_send("key esc tap")
        h.wait(2)
        h.interact(0, 65, 2)
        h.wait(3)
        assert _ui(h)["craft_size"] == 3
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.close_screen()
        h.command("setblock 0 65 2 minecraft:air")
        h.command("setblock 1 65 2 minecraft:air")
        h.wait(2)


def test_120_scrub_furnace_cooking_progress_client_sync(h):
    """Open Furnace client flame/arrow counters scrub smoothly backwards, forwards, and match server on commit."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command(
        "setblock 1 65 2 minecraft:furnace"
        "{Items:[{Slot:0b,id:'minecraft:raw_iron',count:8},{Slot:1b,id:'minecraft:coal',count:4}]}"
    )
    h.wait(4)
    h.rreturn("scrub on")
    h.wait(8)

    try:
        h.interact(1, 65, 2)
        h.wait(5)
        ui_start = _ui(h)
        assert ui_start["screen"] == "inventory" and ui_start["container"] == [1, 65, 2], ui_start
        assert h.screen().get("screen") == "container", h.screen()
        # Record ~60 ticks while furnace actively cooks raw iron
        h.wait(60)

        ui_live = _ui(h)
        be_live = ui_live.get("container_be")
        assert isinstance(be_live, dict) and be_live.get("kind") == "furnace", f"Missing furnace BE in UI state: {ui_live}"
        live_cook = be_live["cooking_timer"]
        live_lit = be_live["lit_remaining"]
        assert live_cook >= 50, f"Furnace did not progress live during open GUI: {be_live}"

        # Enter scrub inside open Furnace and step backward -40 ticks
        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -40")
        h.wait(2)
        ui_back = _ui(h)
        assert ui_back["scrubbing"] is True and ui_back["screen"] == "inventory" and ui_back["container"] == [1, 65, 2], ui_back
        assert h.screen().get("screen") == "container", h.screen()
        be_back = ui_back.get("container_be")
        assert isinstance(be_back, dict) and be_back.get("kind") == "furnace", ui_back
        back_cook = be_back["cooking_timer"]
        back_lit = be_back["lit_remaining"]
        assert back_cook <= live_cook - 25 and back_cook >= 5, (
            f"Furnace arrow progress froze instead of rewinding: live={live_cook}, rewound={back_cook}, be={be_back}"
        )
        assert back_lit >= live_lit + 25, (
            f"Furnace flame remaining did not rewind: live={live_lit}, rewound={back_lit}"
        )

        # Scrub forward +20 ticks -> cooking_timer must increase smoothly!
        h.rreturn("scrub seek_ticks 20")
        h.wait(2)
        be_fwd = _ui(h).get("container_be")
        assert isinstance(be_fwd, dict), _ui(h)
        fwd_cook = be_fwd["cooking_timer"]
        assert fwd_cook >= back_cook + 12 and fwd_cook <= live_cook - 10, (
            f"Furnace forward scrub did not advance cooking_timer smoothly: back={back_cook}, fwd={fwd_cook}, live={live_cook}"
        )

        # Commit at rewound position and verify client & server agree immediately
        h.raw_send("key enter tap")
        h.wait(3)
        ui_comm = _ui(h)
        assert ui_comm["scrubbing"] is False and ui_comm["screen"] == "inventory" and ui_comm["container"] == [1, 65, 2], ui_comm
        assert h.screen().get("screen") == "container", h.screen()
        be_comm = ui_comm.get("container_be")
        assert isinstance(be_comm, dict) and be_comm.get("kind") == "furnace", ui_comm
        assert abs(be_comm["cooking_timer"] - fwd_cook) <= 10, (
            f"Post-commit furnace cooking_timer mismatch: expected ~{fwd_cook}, got {be_comm}"
        )
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.close_screen()
        h.command("setblock 1 65 2 minecraft:air")
        h.wait(2)


def test_121_scrub_creative_survival_hud_mode_and_xp_transitions(h):
    """Scrubbing across Creative <-> Survival updates client HUD mode (no Survival hearts in Creative) and XP."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.rreturn("scrub on")
    h.wait(6)

    try:
        # Phase 1: Creative with 0 XP
        h.command("xp set @s 0 levels")
        h.command("gamemode creative")
        h.wait(20)
        ui_cr = _ui(h)
        assert ui_cr["creative"] is True and ui_cr["client_game_mode"] == 1, ui_cr

        # Phase 2: Switch to Survival and gain XP
        h.command("gamemode survival")
        h.command("xp set @s 7 levels")
        h.wait(20)
        ui_sv = _ui(h)
        assert ui_sv["creative"] is False and ui_sv["client_game_mode"] == 0 and ui_sv["xp_level"] == 7, ui_sv

        # Scrub back to Creative phase (-30 ticks): HUD must switch back to Creative (no hearts/XP)!
        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -30")
        h.wait(2)
        ui_rewound_cr = _ui(h)
        assert ui_rewound_cr["scrubbing"] is True
        assert ui_rewound_cr["creative"] is True and ui_rewound_cr["client_game_mode"] == 1, (
            f"Client remained in Survival HUD when scrubbed back to Creative: {ui_rewound_cr}"
        )
        assert h.state().get("gamemode") == "creative", h.state()

        # Scrub forward into Survival + 7 XP phase (+25 ticks)
        h.rreturn("scrub seek_ticks 25")
        h.wait(2)
        ui_fwd_sv = _ui(h)
        assert ui_fwd_sv["creative"] is False and ui_fwd_sv["client_game_mode"] == 0, ui_fwd_sv
        assert ui_fwd_sv["xp_level"] == 7, f"Expected scrubbed client XP level 7: {ui_fwd_sv}"

        # Cancel returns cleanly to Survival
        h.raw_send("key esc tap")
        h.wait(2)
        ui_canc = _ui(h)
        assert ui_canc["scrubbing"] is False
        assert ui_canc["creative"] is False and ui_canc["client_game_mode"] == 0 and ui_canc["xp_level"] == 7, ui_canc

        # Re-enter scrub and commit back into Creative: client must stay Creative with instabuild intact!
        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -30")
        h.wait(2)
        h.raw_send("key enter tap")
        h.wait(4)
        ui_comm_cr = _ui(h)
        assert ui_comm_cr["scrubbing"] is False
        assert ui_comm_cr["creative"] is True and ui_comm_cr["client_game_mode"] == 1, (
            f"Committed rollback to Creative lost creative/instabuild flag: {ui_comm_cr}"
        )
        assert h.state().get("gamemode") == "creative", h.state()
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.command("gamemode survival")
        h.command("xp set @s 0 levels")
        h.wait(2)


def test_122_scrub_f3_f4_gamemode_switcher_display(h):
    """Scrubbing over recorded F3+F4 GameModeSwitcher frames displays the popup with exact selection & HUD mode."""
    orig_world = h.world
    cmds_world = f"{orig_world}_f3f4_{h.current_test_id}"
    h.open_world(cmds_world, seed=12345, mode="creative")

    try:
        h.rreturn("dev on")
        h.rreturn("scrub on")
        h.command("fill -3 64 -3 3 64 3 minecraft:stone")
        h.command("fill -3 65 -3 3 68 3 minecraft:air")
        h.tp(0.5, 65.0, 0.5)
        h.wait(10)
        assert h.state().get("gamemode") == "creative"

        # Hold F3 and tap F4 twice -> Creative (1) -> Survival (0) -> Adventure (2)
        h.raw_send("key f3 down")
        h.raw_send("key f4 tap")
        h.wait(8)
        ui_sw1 = _ui(h)
        assert h.screen().get("screen") == "gamemode-switcher", h.screen()
        assert ui_sw1["screen"] == "game_mode_switcher" and ui_sw1["gamemode_switcher_selected"] == 0, ui_sw1

        h.raw_send("key f4 tap")
        h.wait(10)
        ui_sw2 = _ui(h)
        assert ui_sw2["screen"] == "game_mode_switcher" and ui_sw2["gamemode_switcher_selected"] == 2, ui_sw2

        # Release F3 -> switches to Adventure, hold 10 ticks
        h.raw_send("key f3 up")
        h.wait(10)
        assert h.state().get("gamemode") == "adventure", h.state()

        # Enter scrub and seek back into the second F3+F4 hold window (-15 ticks)
        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -15")
        h.wait(2)
        ui_preview2 = _ui(h)
        assert ui_preview2["scrubbing"] is True
        assert h.screen().get("screen") == "gamemode-switcher", f"F3+F4 menu missing during scrub: {h.screen()}"
        assert ui_preview2["screen"] == "game_mode_switcher", ui_preview2
        assert ui_preview2["gamemode_switcher_selected"] == 2, ui_preview2
        assert ui_preview2["creative"] is True and ui_preview2["client_game_mode"] == 1, ui_preview2

        # Seek slightly further back into the first F3+F4 hold window (-8 more ticks)
        h.rreturn("scrub seek_ticks -8")
        h.wait(2)
        ui_preview1 = _ui(h)
        assert ui_preview1["screen"] == "game_mode_switcher", ui_preview1
        assert ui_preview1["gamemode_switcher_selected"] == 0, ui_preview1
        assert ui_preview1["creative"] is True, ui_preview1

        # Cancel scrub returns to Adventure playing screen cleanly
        h.raw_send("key esc tap")
        h.wait(2)
        ui_end = _ui(h)
        assert ui_end["scrubbing"] is False and ui_end["screen"] == "playing", ui_end
        assert h.state().get("gamemode") == "adventure", h.state()
    finally:
        try:
            if _ui(h).get("scrubbing"):
                h.rreturn("scrub cancel")
            h.rreturn("scrub off")
        except Exception:
            pass
        h.open_world(orig_world, seed=12345)


def test_125_chat_history_rollback_and_scrub(h):
    """Rollback (F6/F7 + disk purge) and Scrubbing (U) both preserve prior chat history and rewind post-point messages."""
    setup_clean_env(h)
    h.rreturn("dev on")
    h.rreturn("scrub on")
    h.tp(0.5, 65.0, 0.5)
    h.wait(5)

    def chat_texts():
        msgs = h.raw_send("messages 40").get("chat", [])
        ui_log = _ui(h).get("chat_log", [])
        return msgs, ui_log

    try:
        # 1. Establish initial chat messages before checkpoint
        h.raw_send("chat alpha_before_cp")
        h.command("say srv_before_cp")
        h.wait(5)
        msgs_pre, log_pre = chat_texts()
        assert any("alpha_before_cp" in m for m in msgs_pre), f"Missing alpha_before_cp in messages: {msgs_pre}"
        assert any("alpha_before_cp" in m for m in log_pre), f"Missing alpha_before_cp in chat_log: {log_pre}"
        assert any("srv_before_cp" in m for m in msgs_pre), f"Missing srv_before_cp in messages: {msgs_pre}"
        assert any("srv_before_cp" in m for m in log_pre), f"Missing srv_before_cp in chat_log: {log_pre}"

        # 2. Save checkpoint (F6 / set) -> add post-checkpoint chat & command messages
        h.rreturn("set")
        h.wait(10)
        h.raw_send("chat beta_after_cp")
        h.command("say srv_after_cp")
        h.wait(5)
        msgs_post, log_post = chat_texts()
        assert any("beta_after_cp" in m for m in msgs_post), msgs_post
        assert any("srv_after_cp" in m for m in log_post), log_post
        assert "beta_after_cp" in _ui(h).get("chat_recent", []), _ui(h)

        # 3. Rollback after full purge_memory -> pre-CP chat preserved, post-CP chat rewound!
        h.rreturn("purge_memory")
        h.rreturn("load")
        h.wait(5)
        msgs_rb, log_rb = chat_texts()
        recent_rb = _ui(h).get("chat_recent", [])
        assert any("alpha_before_cp" in m for m in msgs_rb), f"Pre-CP chat lost from messages after F7: {msgs_rb}"
        assert any("alpha_before_cp" in m for m in log_rb), f"Pre-CP chat lost from chat_log after F7: {log_rb}"
        assert any("srv_before_cp" in m for m in msgs_rb), f"Pre-CP server chat lost after F7: {msgs_rb}"
        assert not any("beta_after_cp" in m for m in msgs_rb), f"Post-CP chat leaked in messages after F7: {msgs_rb}"
        assert not any("beta_after_cp" in m for m in log_rb), f"Post-CP chat leaked in chat_log after F7: {log_rb}"
        assert not any("srv_after_cp" in m for m in msgs_rb), f"Post-CP server chat leaked after F7: {msgs_rb}"
        assert "beta_after_cp" not in recent_rb, f"Post-CP sent history leaked after F7: {recent_rb}"

        # 4. Live Scrubbing: preview rewinds chat, Esc cancels (restores future chat), Enter commits (rewinds chat)
        h.wait(10)
        h.raw_send("chat scrub_t1_msg")
        h.wait(28)
        h.raw_send("chat scrub_t2_msg")
        h.wait(12)

        msgs_before_u, log_before_u = chat_texts()
        assert any("scrub_t2_msg" in m for m in msgs_before_u) and any("scrub_t2_msg" in m for m in log_before_u)

        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -24")
        h.wait(3)
        msgs_prev, log_prev = chat_texts()
        assert any("scrub_t1_msg" in m for m in log_prev), f"Expected scrub_t1_msg in scrub preview: {log_prev}"
        assert not any("scrub_t2_msg" in m for m in log_prev), f"Expected scrub_t2_msg rolled back in preview: {log_prev}"
        assert not any("scrub_t2_msg" in m for m in msgs_prev), f"Expected scrub_t2_msg rolled back in messages(): {msgs_prev}"

        # Esc cancels -> future chat returns
        h.raw_send("key esc tap")
        h.wait(3)
        msgs_esc, log_esc = chat_texts()
        assert any("scrub_t2_msg" in m for m in msgs_esc) and any("scrub_t2_msg" in m for m in log_esc), (
            f"Esc cancel did not restore future chat: msgs={msgs_esc} log={log_esc}"
        )

        # Seek again and Enter commits -> future chat permanently rolled back
        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -24")
        h.wait(3)
        h.raw_send("key enter tap")
        h.wait(3)
        msgs_comm, log_comm = chat_texts()
        assert any("scrub_t1_msg" in m for m in log_comm), f"Committed chat lost scrub_t1_msg: {log_comm}"
        assert not any("scrub_t2_msg" in m for m in msgs_comm), f"Committed scrub kept future chat in messages: {msgs_comm}"
        assert not any("scrub_t2_msg" in m for m in log_comm), f"Committed scrub kept future chat in log: {log_comm}"
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.wait(2)


def test_126_sign_edit_wheel_and_scrub_hud_drag_lock(h):
    """Mouse wheel & scrub hotkeys keep scrubbing when a Sign Editor frame is active, and HUD stays locked during cursor drag."""
    setup_clean_env(h)
    h.rreturn("scrub on")
    h.command("fill -2 64 -2 2 64 2 minecraft:stone")
    h.command("fill -2 65 -2 2 67 2 minecraft:air")
    h.command("setblock 0 65 2 minecraft:oak_sign")
    h.tp(0.5, 65.0, 0.5)
    h.wait(4)

    try:
        # ---- Part A: Sign Editor active during Scrub must NOT swallow scroll-at wheel or keys ----
        h.interact(0, 65, 2)
        h.wait(4)
        ui_sign = _ui(h)
        assert ui_sign.get("sign_edit") is not None, f"Sign editor failed to open: {ui_sign}"
        h.raw_send("type ScrubSign")
        h.wait(16)
        assert _ui(h)["sign_edit"]["edit_value"] == "ScrubSign", _ui(h)

        # Finish editing sign and record ticks after closing
        h.raw_send("key esc tap")
        h.wait(16)
        assert _ui(h).get("sign_edit") is None, _ui(h)

        # Start scrubbing and seek back into open sign_edit window
        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -22")
        h.wait(3)
        ui_in_sign = _ui(h)
        assert ui_in_sign["scrubbing"] is True and ui_in_sign.get("sign_edit") is not None, (
            f"Expected sign_edit restored in scrub preview: {ui_in_sign}"
        )
        t_before_wheel = float(h.rreturn("scrub status")["current_time"])

        # Send REAL window mouse wheel while sign editor is on screen!
        h.raw_send("scroll-at 100 100 -2")
        h.wait(2)
        t_after_back = float(h.rreturn("scrub status")["current_time"])
        assert t_after_back < t_before_wheel - 0.5, (
            f"Mouse wheel failed to rewind while sign editor was active during scrub: "
            f"before={t_before_wheel} after={t_after_back}"
        )

        h.raw_send("scroll-at 100 100 1")
        h.wait(2)
        t_after_fwd = float(h.rreturn("scrub status")["current_time"])
        assert t_after_fwd > t_after_back + 0.5, (
            f"Mouse wheel failed to advance while sign editor was active during scrub: "
            f"back={t_after_back} fwd={t_after_fwd}"
        )

        # Normal character keystrokes while scrubbing on sign_edit frame must not edit the sign
        h.raw_send("type X")
        h.wait(2)
        assert _ui(h)["sign_edit"]["edit_value"] == "ScrubSign", (
            f"Typing during scrub modified sign text: {_ui(h)['sign_edit']}"
        )
        h.raw_send("key esc tap")
        h.wait(3)
        assert _ui(h)["scrubbing"] is False, "Esc failed to cancel scrub while sign_edit was shown"

        # ---- Part B: Playing <-> Inventory layout transitions work on wheel/keys, lock during cursor track drag ----
        h.rreturn("scrub off")
        h.wait(2)
        h.rreturn("scrub on")
        h.wait(20)
        h.raw_send("key e tap")
        h.wait(20)
        h.raw_send("key esc tap")
        h.wait(18)

        h.raw_send("key u tap")
        h.wait(2)
        hud_play = h.rreturn("ui scrub_hud")
        assert hud_play["capsule"][3] > 30.0, f"Expected full 2-row Playing HUD initially: {hud_play}"

        # Stepping with wheel into Inventory frame MUST move/compact the HUD (not locked!)
        h.rreturn("scrub seek_ticks -25")
        h.wait(2)
        assert _ui(h)["screen"] == "inventory"
        hud_inv_wheel = h.rreturn("ui scrub_hud")
        assert hud_inv_wheel["capsule"][3] <= 24.0, (
            f"Wheel/key scrub into Inventory must still switch to compact HUD: {hud_inv_wheel}"
        )
        h.rreturn("scrub cancel")
        h.wait(2)

        # Re-open scrub at Playing, check drag-at across track works cleanly without jumping
        h.raw_send("key u tap")
        h.wait(2)
        hud_start = h.rreturn("ui scrub_hud")
        tr = hud_start["track"]
        tx0 = int(round(tr[0] + tr[2] * 0.92))
        tx1 = int(round(tr[0] + tr[2] * 0.52))
        ty = int(round(hud_start["track_center"][1]))
        h.raw_send(f"drag-at {tx0} {ty} {tx1} {ty}")
        h.wait(2)
        hud_after_drag = h.rreturn("ui scrub_hud")
        assert hud_after_drag.get("locked") is False, f"HUD remained locked after cursor release: {hud_after_drag}"
        assert _ui(h)["screen"] == "inventory" and hud_after_drag["capsule"][3] <= 24.0, (
            f"Expected release after drag into Inventory to settle into compact HUD: {hud_after_drag}"
        )
        h.raw_send("key esc tap")
        h.wait(2)
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.command("setblock 0 65 2 minecraft:air")
        h.close_screen()
        h.wait(2)


def test_131_f5_perspective_save_rollback_and_scrub(h):
    """F5 camera perspective (first-person / 3rd back / 3rd front) must save and rollback via F6/F7 RAM, disk, and Scrub."""
    setup_clean_env(h)
    try:
        # Ensure we start at first-person (0)
        for _ in range(4):
            if _ui(h)["perspective"] == 0:
                break
            h.raw_send("key f5 tap")
            h.wait(2)
        assert _ui(h)["perspective"] == 0 and _ui(h)["perspective_name"] == "first_person", _ui(h)

        # Press F5 -> third_person_back (1) & save
        h.raw_send("key f5 tap")
        h.wait(2)
        assert _ui(h)["perspective"] == 1 and _ui(h)["perspective_name"] == "third_person_back", _ui(h)
        h.rreturn("save persp_t1")

        # Press F5 -> third_person_front (2) & save
        h.raw_send("key f5 tap")
        h.wait(2)
        assert _ui(h)["perspective"] == 2 and _ui(h)["perspective_name"] == "third_person_front", _ui(h)
        h.rreturn("save persp_t2")

        # Press F5 -> back to 0
        h.raw_send("key f5 tap")
        h.wait(2)
        assert _ui(h)["perspective"] == 0

        # F7 RAM load persp_t1 -> 1
        h.rreturn("load persp_t1")
        h.wait(2)
        assert _ui(h)["perspective"] == 1, f"Expected 3rd person back after RAM load: {_ui(h)}"

        # F7 disk load after purge_memory persp_t2 -> 2
        h.rreturn("purge_memory")
        h.wait(2)
        h.rreturn("load persp_t2")
        h.wait(2)
        assert _ui(h)["perspective"] == 2, f"Expected 3rd person front after disk load: {_ui(h)}"

        # Reset to 0 and record 0 -> 1 -> 2 across scrub timeline synced to game_time
        h.raw_send("key f5 tap")
        h.wait(2)
        assert _ui(h)["perspective"] == 0
        h.rreturn("scrub on")
        t0_start = _tick(h)
        h.wait(18)
        t0_end = _tick(h)
        h.raw_send("key f5 tap")
        h.wait(4)
        assert _ui(h)["perspective"] == 1
        t1_start = _tick(h)
        h.wait(18)
        t1_end = _tick(h)
        h.raw_send("key f5 tap")
        h.wait(4)
        assert _ui(h)["perspective"] == 2
        h.wait(14)

        t0_mid = (t0_start + t0_end) // 2
        t1_mid = (t1_start + t1_end) // 2

        h.raw_send("key u tap")
        h.wait(2)
        assert _ui(h)["scrubbing"] is True and _ui(h)["perspective"] == 2

        _scrub_seek_to_tick(h, t1_mid)
        h.wait(2)
        assert _ui(h)["perspective"] == 1, f"Scrub preview did not restore perspective 1: {_ui(h)}"

        _scrub_seek_to_tick(h, t0_mid)
        h.wait(2)
        assert _ui(h)["perspective"] == 0, f"Scrub preview did not restore perspective 0: {_ui(h)}"

        # Cancel must return to anchor perspective 2
        h.raw_send("key esc tap")
        h.wait(2)
        assert _ui(h)["scrubbing"] is False and _ui(h)["perspective"] == 2, (
            f"Scrub cancel failed to restore anchor perspective 2: {_ui(h)}"
        )

        # Re-open scrub -> seek to 1 -> commit -> stays 1
        h.raw_send("key u tap")
        h.wait(2)
        _scrub_seek_to_tick(h, t1_mid)
        h.wait(2)
        assert _ui(h)["perspective"] == 1
        h.raw_send("key enter tap")
        h.wait(4)
        assert _ui(h)["scrubbing"] is False and _ui(h)["perspective"] == 1, (
            f"Scrub commit did not keep perspective 1: {_ui(h)}"
        )
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        for _ in range(4):
            if _ui(h)["perspective"] == 0:
                break
            h.raw_send("key f5 tap")
            h.wait(2)


def test_132_creative_hotbar_direct_place_clears_hand_on_scrub(h):
    """Item placed directly from creative menu into active hotbar slot must clear from visible hand on scrub."""
    setup_clean_env(h)
    try:
        h.command("gamemode creative")
        h.command("clear @s")
        h.select_slot(0)
        h.wait(4)
        assert _ui(h)["visible_main_hand"] is None, f"Expected empty hand initially: {_ui(h)}"

        h.rreturn("scrub on")
        h.wait(14)

        # Open Creative inventory, pick block from colored_blocks, place directly into active hotbar slot 0
        h.raw_send("key e tap")
        h.wait(3)
        ui_tabs = _ui(h)
        cb_pos = ui_tabs["creative_tabs"]["minecraft:colored_blocks"]
        h.raw_send(f"click-at {cb_pos[0]} {cb_pos[1]}")
        h.wait(2)
        slot0 = _ui(h)["creative_slot0"]
        h.raw_send(f"click-at {slot0[0]} {slot0[1]}")
        h.wait(2)
        picked = _ui(h)["carried"]
        assert picked is not None, f"Failed to pick item in creative menu: {_ui(h)}"

        scr = h.screen()
        hotbar0_rect = None
        for sl in scr.get("slots", []):
            if sl.get("id") == 0 and isinstance(sl.get("rect"), list) and len(sl["rect"]) == 4:
                hotbar0_rect = sl["rect"]
                break
        if hotbar0_rect is not None:
            hx = int(round(hotbar0_rect[0] + hotbar0_rect[2] * 0.5))
            hy = int(round(hotbar0_rect[1] + hotbar0_rect[3] * 0.5))
            h.raw_send(f"click-at {hx} {hy}")
        else:
            h.click_slot(0)
        h.wait(8)

        # Close creative and wait a bit so visible_hands reflects held item in first person
        h.raw_send("key esc tap")
        h.wait(10)
        ui_held = _ui(h)
        assert ui_held["visible_main_hand"] is not None, (
            f"Expected placed creative item in visible_main_hand: {ui_held}"
        )

        # Enter scrub and rewind back before creative menu opened
        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -45")
        h.wait(2)

        ui_prev = _ui(h)
        assert ui_prev["scrubbing"] is True
        assert ui_prev["visible_main_hand"] is None, (
            f"Creative hotbar item remained in visible hand during scrub preview! {ui_prev}"
        )
        inv_prev = h.inventory().get("hotbar", [])
        assert inv_prev and inv_prev[0]["item"] is None, f"Hotbar slot 0 not empty on scrub preview: {inv_prev}"

        # Scrub forward into active held state -> visible_main_hand must reappear!
        h.rreturn("scrub seek_ticks 40")
        h.wait(2)
        assert _ui(h)["visible_main_hand"] is not None, (
            f"Scrub forward failed to restore creative hand item: {_ui(h)}"
        )

        # Scrub backward again & commit -> must stay clear on hand after normal ticks resume!
        h.rreturn("scrub seek_ticks -45")
        h.wait(2)
        assert _ui(h)["visible_main_hand"] is None
        h.raw_send("key enter tap")
        h.wait(8)
        ui_comm = _ui(h)
        assert ui_comm["scrubbing"] is False
        assert ui_comm["visible_main_hand"] is None, (
            f"Creative item returned or stayed in visible hand after scrub commit! {ui_comm}"
        )
        inv_comm = h.inventory().get("hotbar", [])
        assert inv_comm and inv_comm[0]["item"] is None, f"Hotbar slot 0 refilled after commit: {inv_comm}"

        # Case B: Trigger scrub IMMEDIATELY while Creative screen is still open right after direct drop into active slot 0
        h.rreturn("scrub off")
        h.wait(2)
        h.command("clear @s")
        h.select_slot(0)
        h.wait(2)
        h.rreturn("scrub on")
        h.wait(14)
        h.raw_send("key e tap")
        h.wait(3)
        cb_pos = _ui(h)["creative_tabs"]["minecraft:colored_blocks"]
        h.raw_send(f"click-at {cb_pos[0]} {cb_pos[1]}")
        h.wait(2)
        slot0 = _ui(h)["creative_slot0"]
        h.raw_send(f"click-at {slot0[0]} {slot0[1]}")
        h.wait(2)
        h.click_slot(0)
        h.wait(8)
        assert _ui(h)["visible_main_hand"] is not None, f"Expected visible hand filled inside Creative: {_ui(h)}"

        # Press U directly inside Creative without closing screen -> rewind before placing -> commit!
        h.raw_send("key u tap")
        h.wait(2)
        h.rreturn("scrub seek_ticks -32")
        h.wait(2)
        assert _ui(h)["visible_main_hand"] is None, (
            f"Inside-creative scrub preview kept item in hand! {_ui(h)}"
        )
        h.raw_send("key enter tap")
        h.wait(6)
        assert _ui(h)["scrubbing"] is False
        assert _ui(h)["visible_main_hand"] is None, (
            f"Inside-creative scrub commit left item in visible hand! {_ui(h)}"
        )
        inv_b = h.inventory().get("hotbar", [])
        assert inv_b and inv_b[0]["item"] is None, inv_b
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.close_screen()
        h.command("clear @s")
        h.command("gamemode survival")
        h.wait(2)


def test_133_virtual_cursor_scrub_and_commit_warp(h):
    """During scrub virtual cursor shows historical GUI pointer position and commit warps real cursor there."""
    setup_clean_env(h)
    try:
        h.command("gamemode creative")
        h.rreturn("scrub on")
        h.wait(6)

        h.raw_send("key e tap")
        h.wait(3)
        ui_open = _ui(h)
        cb_pos = ui_open["creative_tabs"]["minecraft:colored_blocks"]
        cx1, cy1 = int(round(cb_pos[0])), int(round(cb_pos[1]))
        h.raw_send(f"click-at {cx1} {cy1}")
        h.wait(2)
        t1_start = _tick(h)
        h.wait(14)
        t1_end = _tick(h)
        ui_t1 = _ui(h)
        assert abs(ui_t1["cursor"][0] - cx1) <= 2.0 and abs(ui_t1["cursor"][1] - cy1) <= 2.0, ui_t1

        # Move pointer to a distinct position elsewhere in the window without closing Creative
        cx2, cy2 = cx1 + 90, cy1 + 55
        h.raw_send(f"scroll-at {cx2} {cy2} 0")
        h.wait(16)
        ui_t2 = _ui(h)
        assert abs(ui_t2["cursor"][0] - cx2) <= 2.0 and abs(ui_t2["cursor"][1] - cy2) <= 2.0, ui_t2

        # Enter scrub and seek backward into T1 (cursor at cx1, cy1) synced to game_time
        t1_mid = (t1_start + t1_end) // 2
        h.raw_send("key u tap")
        h.wait(2)
        _scrub_seek_to_tick(h, t1_mid)
        h.wait(2)

        ui_prev = _ui(h)
        assert ui_prev["scrubbing"] is True
        vc = ui_prev.get("virtual_cursor")
        assert isinstance(vc, list) and len(vc) == 2, f"Expected virtual_cursor during scrub: {ui_prev}"
        assert abs(vc[0] - cx1) <= 3.0 and abs(vc[1] - cy1) <= 3.0, (
            f"Virtual cursor mismatch: expected ~({cx1}, {cy1}), got {vc}"
        )

        # Click HUD Apply pill (far from cx1, cy1) -> moves real mouse to apply_center & commits!
        hud = h.rreturn("ui scrub_hud")
        ax, ay = int(round(hud["apply_center"][0])), int(round(hud["apply_center"][1]))
        assert abs(ax - cx1) > 10 or abs(ay - cy1) > 10, f"Apply pill too close to test coords: {hud}"
        h.raw_send(f"click-at {ax} {ay}")
        h.wait(4)

        ui_done = _ui(h)
        assert ui_done["scrubbing"] is False, f"Apply click did not commit scrub: {ui_done}"
        assert ui_done.get("virtual_cursor") is None, f"Virtual cursor stayed active after commit: {ui_done}"
        assert ui_done["pointer_captured"] is False, ui_done
        cur = ui_done["cursor"]
        assert abs(cur[0] - cx1) <= 3.0 and abs(cur[1] - cy1) <= 3.0, (
            f"Real cursor did not warp to virtual cursor position ({cx1}, {cy1}) on commit! got {cur}"
        )
    finally:
        if _ui(h).get("scrubbing"):
            h.rreturn("scrub cancel")
        h.rreturn("scrub off")
        h.close_screen()
        h.command("clear @s")
        h.command("gamemode survival")
        h.wait(2)
