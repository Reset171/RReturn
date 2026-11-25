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
        # 1. Start scrub with U key
        h.raw_send("key u tap")
        h.wait(2)
        ui = _ui(h)
        assert ui["scrubbing"] is True, f"Scrub did not start on U: {ui}"

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

        # 4. Enter commits scrub cleanly
        h.raw_send("key enter tap")
        h.wait(3)
        ui_commit = _ui(h)
        assert ui_commit["scrubbing"] is False, f"Enter failed to commit scrub: {ui_commit}"
        assert h.screen().get("screen") == "playing"

        # 5. Re-enter scrub and Esc cancels cleanly
        h.wait(8)
        h.raw_send("key u tap")
        h.wait(2)
        assert _ui(h)["scrubbing"] is True
        h.raw_send("key left tap")
        h.wait(2)
        h.raw_send("key esc tap")
        h.wait(3)
        ui_cancel = _ui(h)
        assert ui_cancel["scrubbing"] is False, f"Esc failed to cancel scrub: {ui_cancel}"
        assert h.screen().get("screen") == "playing"
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
