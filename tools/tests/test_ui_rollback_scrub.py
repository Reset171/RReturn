"""Tests 112-114: Multi-screen hotkeys, exact UI/cursor/chat restore, and reversible UI scrubbing."""

import tempfile
from pathlib import Path
from . import setup_clean_env

SCRATCH_DIR = Path(tempfile.gettempdir()) / "rreturn_e2e_screenshots"


def _ui(h):
    st = h.rreturn("ui state")
    assert isinstance(st, dict), f"Expected JSON from 'rreturn ui state', got: {st}"
    return st


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
