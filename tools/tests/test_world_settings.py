"""Tests 19-23: World Settings, Commands, Tick Rate, Weather, Biomes, Schedule."""

import json
import tempfile
import time
from pathlib import Path
from tools.rreturn_harness import default_saves_dir
from . import setup_clean_env


def _global_settings_path(h):
    instance_dir = (default_saves_dir(h.instance) / h.world).parent.parent
    return instance_dir / "config" / "rreturn.json"


def _world_settings_path(h):
    return default_saves_dir(h.instance) / h.world / "rreturn" / "settings.json"


def test_19_tick_rate_and_freeze_rollback(h):
    setup_clean_env(h)
    h.command("tick unfreeze")
    h.command("tick rate 20")
    h.wait(3)

    h.rreturn("set")
    h.wait(2)

    h.command("tick rate 45")
    h.command("tick freeze")
    h.wait(2)

    h.rreturn("load")
    h.wait(4)

    q = h.command("tick query")
    chat = q.get("chat", [])
    assert any("20.0" in line for line in chat), f"Tick rate not rolled back: {chat}"
    assert any(
        "обычном режиме" in line.lower() or "запущена" in line.lower() or "running" in line.lower()
        for line in chat
    ), f"Tick freeze not rolled back: {chat}"

    t0_day = h.state().get("time", {}).get("day_time", 0)
    h.wait(10)
    t1_day = h.state().get("time", {}).get("day_time", 0)
    diff_day = t1_day - t0_day
    assert 5 <= diff_day <= 25, f"Sun/day cycle delta {diff_day} ticks over 10 game ticks is abnormal"


def test_20_world_spawn_and_difficulty_lock(h):
    setup_clean_env(h)
    h.command("difficulty normal")
    h.command("setworldspawn 100 70 100")
    h.wait(3)

    h.rreturn("set")
    h.wait(2)

    h.command("difficulty hard")
    h.command("setworldspawn 800 80 800")
    h.wait(3)

    d_hard = h.command("difficulty").get("chat", [])
    assert any("сложная" in line.lower() or "hard" in line.lower() for line in d_hard), f"Difficulty not set to hard: {d_hard}"

    h.rreturn("load")
    h.wait(4)

    d_res = h.command("difficulty")
    d_chat = d_res.get("chat", [])
    assert any("нормальная" in line.lower() or "normal" in line.lower() for line in d_chat), f"Difficulty not rolled back: {d_chat}"


def test_21_schedule_function_cancellation(h):
    setup_clean_env(h)
    h.tp(0.5, 65, 0.5)
    h.command("setblock 0 65 2 minecraft:air")
    h.command("setblock 0 65 3 minecraft:air")
    h.wait(2)

    h.rreturn("set")
    h.wait(2)

    h.command("setblock 0 65 2 minecraft:repeater[delay=4,powered=true]")
    h.command("schedule function rreturn:future_ping 1000t")
    h.wait(2)

    h.rreturn("load")
    h.wait(6)

    b_rep = h.block(0, 65, 2)
    assert b_rep.get("block") == "minecraft:air", f"Scheduled repeater block tick leaked: {b_rep}"
    clear_res = h.command("schedule clear rreturn:future_ping")
    chat = clear_res.get("chat", [])
    assert any(
        "нет запланированных" in line.lower()
        or "не найдено" in line.lower()
        or "не существует" in line.lower()
        or "no schedules" in line.lower()
        or "fail" in line.lower()
        for line in chat
    ), f"Scheduled function was not cancelled: {chat}"


def test_22_fillbiome_rollback(h):
    setup_clean_env(h)
    h.tp(500.5, 65, 500.5)
    h.command("fillbiome 495 60 495 505 75 505 minecraft:plains")
    h.wait(3)
    assert h.check_if("biome 500 65 500 minecraft:plains"), "Initial biome was not set to plains"

    h.rreturn("set")
    h.wait(2)

    h.command("fillbiome 495 60 495 505 75 505 minecraft:cherry_grove")
    h.wait(3)
    assert h.check_if("biome 500 65 500 minecraft:cherry_grove"), "Biome did not change to cherry_grove before rollback"

    h.rreturn("load")
    h.wait(6)

    assert h.check_if("biome 500 65 500 minecraft:plains"), "Biome was not rolled back to plains"
    assert not h.check_if("biome 500 65 500 minecraft:cherry_grove"), "Future cherry_grove biome leaked after rollback"


def test_23_weather_and_thunder_rollback(h):
    setup_clean_env(h)
    h.command("weather clear")
    h.wait(4)

    h.rreturn("set")
    h.wait(2)

    h.command("weather thunder")
    for _ in range(6):
        h.wait(2)
        st_thunder = h.state().get("weather", {})
        if st_thunder.get("thundering", False):
            break
    assert st_thunder.get("thundering", False), f"Thunder weather was not set: {st_thunder}"

    h.rreturn("load")
    h.wait(4)

    st_clear = h.state().get("weather", {})
    assert not st_clear.get("thundering", False), f"Thunder weather not rolled back: {st_clear}"
    assert not st_clear.get("raining", False), f"Rain weather not rolled back: {st_clear}"


def test_105_dev_mode_chat_filtering(h):
    setup_clean_env(h)

    # 1. Dev mode is currently ON (set by setup_clean_env).
    # Commands should output chat messages.
    res_on = h.command("rreturn status")
    assert len(res_on.get("chat", [])) > 0, f"Expected chat messages when dev mode is ON: {res_on}"

    # 2. Turn dev mode OFF. The dev command itself still reports its status in chat.
    res_dev_off = h.command("rreturn dev off")
    chat_off = res_dev_off.get("chat", [])
    assert len(chat_off) > 0, f"Expected dev toggle response in chat: {res_dev_off}"
    assert "off" in chat_off[0].lower() or "выкл" in chat_off[0].lower(), f"Unexpected dev off message: {chat_off}"

    # 3. With dev mode OFF, subsequent commands must NOT output to chat.
    res_silent_status = h.command("rreturn status")
    chat_after_status = res_silent_status.get("chat", [])
    assert chat_after_status == chat_off, (
        f"New chat message was added while dev mode was OFF: {chat_after_status}"
    )

    res_silent_save = h.command("rreturn set")
    chat_after_save = res_silent_save.get("chat", [])
    assert chat_after_save == chat_off, (
        f"Save message was added to chat while dev mode was OFF: {chat_after_save}"
    )

    res_silent_scrub = h.command("rreturn scrub status")
    chat_after_scrub = res_silent_scrub.get("chat", [])
    assert chat_after_scrub == chat_off, (
        f"Scrub status was added to chat while dev mode was OFF: {chat_after_scrub}"
    )

    # 4. Turn dev mode back ON.
    res_dev_on = h.command("rreturn dev on")
    chat_on = res_dev_on.get("chat", [])
    assert len(chat_on) > 0, f"Expected dev toggle response in chat: {res_dev_on}"
    assert "on" in chat_on[0].lower() or "вкл" in chat_on[0].lower(), f"Unexpected dev on message: {chat_on}"

    # 5. Subsequent commands should output to chat again.
    res_after = h.command("rreturn status")
    assert len(res_after.get("chat", [])) > 0, f"Expected chat output after reenabling dev mode: {res_after}"


def test_106_logs_directory_structure(h):
    setup_clean_env(h)

    world_dir = default_saves_dir(h.instance) / h.world
    instance_dir = world_dir.parent.parent

    # Clear old files if any existed from previous runs
    world_old_log = world_dir / "rreturn" / "benchmark.log"
    if world_old_log.exists():
        try:
            world_old_log.unlink()
        except Exception:
            pass
    instance_old_log = instance_dir / "logs" / "rreturn.log"
    if instance_old_log.exists():
        try:
            instance_old_log.unlink()
        except Exception:
            pass

    # Trigger checkpoint creation which calls append_to_rreturn_log
    h.rreturn("set")
    h.wait(4)

    # 1. World-level log must be under saves/<world>/logs/rreturn/benchmark.log
    world_new_log = world_dir / "logs" / "rreturn" / "benchmark.log"
    assert world_new_log.exists(), f"Expected world log at {world_new_log}, but it was not found"
    assert not world_old_log.exists(), f"Old world log at {world_old_log} should NOT exist"

    # 2. Instance-level log must be under <instance>/logs/rreturn/rreturn.log
    if (instance_dir / "logs").is_dir():
        instance_new_log = instance_dir / "logs" / "rreturn" / "rreturn.log"
        assert instance_new_log.exists(), f"Expected instance log at {instance_new_log}, but it was not found"
        assert not instance_old_log.exists(), f"Old instance log at {instance_old_log} should NOT exist"


def test_111_settings_ui_toggles_search_and_persistence(h):
    setup_clean_env(h)
    global_cfg_path = _global_settings_path(h)
    global_before = global_cfg_path.read_text(encoding="utf-8") if global_cfg_path.is_file() else None
    h.rreturn("policy reset")
    h.rreturn("scrub policy reset")
    h.rreturn("player reset @s")
    h.rreturn("anchor clear")
    h.wait(2)

    # 1. Verify keybind K opens Screen::Mod and Esc closes back to Playing
    h.raw_send("key k tap")
    h.wait(2)
    assert h.screen().get("screen") == "mod", f"Expected K keybind to open mod screen, got: {h.screen()}"
    h.raw_send("key esc tap")
    h.wait(2)
    assert h.screen().get("screen") == "playing", f"Expected Esc to close mod screen, got: {h.screen()}"

    # 2. Re-open via /rreturn settings (split dark settings layout)
    h.rreturn("settings")
    h.wait(2)
    assert h.screen().get("screen") == "mod", f"Expected /rreturn settings to open mod screen, got: {h.screen()}"

    try:
        # Query live rendered hitboxes (resolution/GUI-scale independent)
        def ui_pt(tag):
            controls = h.rreturn("ui bounds").get("controls", {})
            assert tag in controls, f"UI control '{tag}' not visible in {sorted(controls.keys())}"
            return controls[tag]

        sx, sy = ui_pt("search")
        h.raw_send(f"click-at {sx} {sy}")
        h.wait(1)
        h.raw_send("type world_weather")
        h.wait(2)

        wx, wy = ui_pt("opt.world_weather")
        h.raw_send(f"click-at {wx} {wy}")
        h.wait(2)

        pol_rb = h.rreturn("policy get")
        pol_sc = h.rreturn("scrub policy get")
        assert pol_rb.get("world_weather") is False, f"Rollback world_weather not toggled: {pol_rb}"
        assert pol_sc.get("world_weather") is True, f"Scrub policy leaked from rollback toggle: {pol_sc}"

        # Clear search, search player_position, toggle scrub.player_position independently
        cx, cy = ui_pt("clear_search")
        h.raw_send(f"click-at {cx} {cy}")
        h.wait(1)
        h.raw_send("type player_position")
        h.wait(2)
        spx, spy = ui_pt("opt.scrub.player_position")
        h.raw_send(f"click-at {spx} {spy}")
        h.wait(2)

        pol_sc_after = h.rreturn("scrub policy get")
        pol_rb_after = h.rreturn("policy get")
        assert pol_sc_after.get("player_position") is False, f"Scrub player_position not toggled: {pol_sc_after}"
        assert pol_rb_after.get("player_position") is True, f"Rollback player_position unexpectedly changed: {pol_rb_after}"

        # Also toggle rollback player_position OFF to verify actual F6/F7 rollback below
        rpx, rpy = ui_pt("opt.player_position")
        h.raw_send(f"click-at {rpx} {rpy}")
        h.wait(2)
        assert h.rreturn("policy get").get("player_position") is False

        # 3. Switch to Players tab and pin the live player in world
        ax, ay = ui_pt("tab.players")
        h.raw_send(f"click-at {ax} {ay}")
        h.wait(2)
        px, py = ui_pt("pin.0")
        h.raw_send(f"click-at {px} {py}")
        h.wait(2)
    finally:
        if h.screen().get("screen") == "mod":
            h.raw_send("key esc tap")
            h.wait(2)
    assert h.screen().get("screen") == "playing"

    # 4. Verify per-world settings.json persisted independent rollback, scrub policies, and pinned player
    #    without overwriting global config/rreturn.json
    cfg_path = _world_settings_path(h)
    assert cfg_path.is_file(), f"Expected persistent world settings at {cfg_path}"
    saved_cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
    assert saved_cfg["checkpointPolicy"]["rollback"]["world"]["weather"] is False
    assert saved_cfg["checkpointPolicy"]["rollback"]["players"]["position"] is False
    assert saved_cfg["scrub"]["policy"]["world"]["weather"] is True
    assert saved_cfg["scrub"]["policy"]["players"]["position"] is False
    assert len(saved_cfg["anchor"]["pinned"]) >= 1, f"Expected pinned player in config: {saved_cfg['anchor']}"
    global_after = global_cfg_path.read_text(encoding="utf-8") if global_cfg_path.is_file() else None
    assert global_after == global_before, "In-world K menu edits must not overwrite global config/rreturn.json"

    # 5. Functional verification: blocks rewind, while weather and player position stay untouched on F6/F7 rollback
    h.tp(0.5, 65.0, 0.5)
    h.command("weather clear")
    h.command("setblock 0 65 3 minecraft:air")
    h.wait(3)
    h.rreturn("set")
    h.wait(3)

    h.command("weather thunder")
    h.command("setblock 0 65 3 minecraft:gold_block")
    h.tp(24.5, 65.0, 24.5)
    pos_before = [0.0, 0.0, 0.0]
    for _ in range(10):
        h.wait(2)
        st_before = h.state()
        pos_before = st_before.get("player", {}).get("position", [0.0, 0.0, 0.0])
        if (
            st_before.get("weather", {}).get("thundering", False)
            and abs(pos_before[0] - 24.5) < 1.5
            and abs(pos_before[2] - 24.5) < 1.5
        ):
            break
    assert h.state().get("weather", {}).get("thundering", False), "Weather did not switch to thunder"
    assert abs(pos_before[0] - 24.5) < 1.5 and abs(pos_before[2] - 24.5) < 1.5, (
        f"Player did not finish teleporting to (24.5, 24.5) before load: {pos_before}"
    )

    h.rreturn("load")
    h.wait(6)

    assert h.block(0, 65, 3).get("block") == "minecraft:air", "Blocks policy was enabled and must rewind"
    assert h.state().get("weather", {}).get("thundering", False), (
        f"Weather rolled back despite world_weather=false: {h.state().get('weather')}"
    )
    pos_after = h.state().get("player", {}).get("position", [0.0, 0.0, 0.0])
    assert abs(pos_after[0] - 24.5) < 1.5 and abs(pos_after[2] - 24.5) < 1.5, (
        f"Player position rolled back despite player_position=false: {pos_after}"
    )

    # Clean up defaults for subsequent tests
    h.rreturn("policy reset")
    h.rreturn("scrub policy reset")
    h.rreturn("anchor clear")
    h.command("weather clear")
    h.tp(0.5, 65.0, 0.5)
    h.wait(3)


def test_115_rreturn_menu_icons_and_per_player_overrides(h):
    """RReturn Menu quick-jump icon buttons, Players tab, and 3-state (Global/Yes/No) per-player rollback & scrub overrides."""
    setup_clean_env(h)
    global_cfg_path = _global_settings_path(h)
    global_before = global_cfg_path.read_text(encoding="utf-8") if global_cfg_path.is_file() else None
    h.rreturn("policy reset")
    h.rreturn("scrub policy reset")
    h.rreturn("player reset all both")
    h.rreturn("anchor clear")
    h.rreturn("autosave adaptive")
    h.rreturn("autosave off")
    h.rreturn("scrub off")
    h.wait(2)

    scratch_dir = Path(tempfile.gettempdir()) / "rreturn_e2e_screenshots"
    scratch_dir.mkdir(parents=True, exist_ok=True)

    def ui_bounds():
        res = h.rreturn("ui bounds")
        assert isinstance(res, dict) and "controls" in res, f"Expected UI bounds dict: {res}"
        return res

    def ui_pt(tag):
        controls = ui_bounds()["controls"]
        assert tag in controls, f"Missing UI hit control '{tag}' in {sorted(controls.keys())}"
        return controls[tag]

    try:
        # 1. Open RReturn Menu on Rollback tab; bottom card / quick action buttons must be removed,
        # and autosave mode sliders must stay hidden while auto_save=false
        h.rreturn("menu rollback")
        h.wait(2)
        assert h.screen().get("screen") == "mod"
        b0 = ui_bounds()
        assert b0["active_tab"] == "rollback", b0
        for gone in ("menu.save_now", "menu.load_now", "menu.toggle_scrub", "save_now", "load_now"):
            assert gone not in b0["controls"], f"Removed control '{gone}' still present: {b0['controls'].keys()}"
        assert "opt.death_rollback" in b0["controls"]
        assert "opt.auto_save" in b0["controls"]
        assert h.rreturn("json").get("death_rollback") is True
        dx0, dy0 = ui_pt("opt.death_rollback")
        h.raw_send(f"click-at {dx0} {dy0}")
        h.wait(2)
        assert h.rreturn("json").get("death_rollback") is False
        h.raw_send(f"click-at {dx0} {dy0}")
        h.wait(2)
        assert h.rreturn("json").get("death_rollback") is True
        assert "slider.fixed_interval_sec" not in b0["controls"], (
            f"Autosave sliders should stay hidden while auto_save is OFF: {b0['controls'].keys()}"
        )

        # Toggle auto_save ON (defaults to Adaptive 2x2 grid) and switch schedule modes cleanly
        ax, ay = ui_pt("opt.auto_save")
        h.raw_send(f"click-at {ax} {ay}")
        h.wait(2)
        b_adap = ui_bounds()
        assert "slider.adaptive_interval_sec" in b_adap["controls"], (
            f"Expected Adaptive top slider after enabling auto_save: {sorted(b_adap['controls'].keys())}"
        )
        assert "slider.fixed_interval_sec" not in b_adap["controls"], b_adap["controls"].keys()
        assert "slider.random_min_sec" not in b_adap["controls"], b_adap["controls"].keys()
        h.raw_send(f"screenshot {scratch_dir / 'menu_rollback.png'}")
        h.wait(1)

        fmx, fmy = ui_pt("autosave.mode.fixed")
        h.raw_send(f"click-at {fmx} {fmy}")
        h.wait(2)
        b_fixed = ui_bounds()
        assert "slider.fixed_interval_sec" in b_fixed["controls"], b_fixed["controls"].keys()
        assert "slider.adaptive_interval_sec" not in b_fixed["controls"], b_fixed["controls"].keys()

        rmx, rmy = ui_pt("autosave.mode.random")
        h.raw_send(f"click-at {rmx} {rmy}")
        h.wait(2)
        b_rand = ui_bounds()
        assert "slider.random_min_sec" in b_rand["controls"], (
            f"Expected Random top slider: {sorted(b_rand['controls'].keys())}"
        )
        assert "slider.fixed_interval_sec" not in b_rand["controls"], b_rand["controls"].keys()

        # Verify System tab omits jump icon buttons and shows real bound hotkeys (without F8)
        sys_x, sys_y = ui_pt("tab.system")
        h.raw_send(f"click-at {sys_x} {sys_y}")
        h.wait(2)
        b_sys = ui_bounds()
        assert b_sys["active_tab"] == "system", b_sys
        assert not any(k.startswith("jump.") for k in b_sys["controls"]), (
            f"System tab should not show quick-jump icons: {b_sys['controls'].keys()}"
        )
        hotkeys = b_sys.get("hotkeys", {})
        assert set(hotkeys.keys()) == {"scrub", "save", "load", "settings_menu"}, f"Unexpected hotkeys: {hotkeys}"
        assert "anchor_menu" not in hotkeys, hotkeys
        assert hotkeys == {"scrub": "U", "save": "F6", "load": "F7", "settings_menu": "K"}, hotkeys

        # Return to Rollback tab, verify newcomer inventory policy defaults to True (enabled = rollback)
        assert h.rreturn("policy get").get("reset_new_player_inventory") is True
        rb_x, rb_y = ui_pt("tab.rollback")
        h.raw_send(f"click-at {rb_x} {rb_y}")
        h.wait(2)

        # Verify reset_new_player_inventory shows up via search and can be searched cleanly
        sx0, sy0 = ui_pt("search")
        h.raw_send(f"click-at {sx0} {sy0}")
        h.wait(1)
        h.raw_send("type reset_new_player_inventory")
        h.wait(2)
        b_srch = ui_bounds()
        assert "opt.reset_new_player_inventory" in b_srch["controls"], b_srch["controls"]
        cx0, cy0 = ui_pt("clear_search")
        h.raw_send(f"click-at {cx0} {cy0}")
        h.wait(2)
        assert h.screen().get("screen") == "mod"

        # Click quick-jump icon to Player section -> must scroll cleanly below top
        jx, jy = ui_pt("jump.rb_player")
        h.raw_send(f"click-at {jx} {jy}")
        h.wait(3)
        b_jump = ui_bounds()
        assert b_jump["scroll_y"] > 50.0, f"Quick-jump icon did not scroll to Player section: {b_jump}"
        assert "opt.player_position" in b_jump["controls"], b_jump["controls"]

        # Click cross-tab Players icon button -> opens Master-Detail Players view directly
        px, py = ui_pt("jump.open_players")
        h.raw_send(f"click-at {px} {py}")
        h.wait(3)
        b_pl = ui_bounds()
        assert b_pl["active_tab"] == "players", f"Expected transition to players tab: {b_pl}"
        assert b_pl["selected_player"] is not None, f"Expected auto-selected player: {b_pl}"
        assert "p_rule.player_position.global" in b_pl["controls"], b_pl["controls"]
        assert "p_rule.player_position.allow" in b_pl["controls"]
        assert "p_rule.player_position.deny" in b_pl["controls"]
        assert not any(k.startswith("jump.players") for k in b_pl["controls"]), b_pl["controls"]
        h.raw_send(f"screenshot {scratch_dir / 'menu_players.png'}")
        h.wait(1)

        # 2. Set @s player_position = Deny (No) for Rollback while Global stays True
        dx, dy = ui_pt("p_rule.player_position.deny")
        h.raw_send(f"click-at {dx} {dy}")
        h.wait(2)
        assert h.rreturn("policy get").get("player_position") is True, "Global policy must remain True"
        p_info_rb = h.rreturn("player get @s rollback")
        assert p_info_rb["data"]["overrides"]["player_position"] is False
        assert p_info_rb["data"]["effective"]["player_position"] is False

        # Also set @s player_position = Deny (No) for Scrub scope
        sx, sy = ui_pt("p_scope.scrub")
        h.raw_send(f"click-at {sx} {sy}")
        h.wait(2)
        sdx, sdy = ui_pt("p_rule.player_position.deny")
        h.raw_send(f"click-at {sdx} {sdy}")
        h.wait(2)
        assert h.rreturn("scrub policy get").get("player_position") is True, "Global scrub policy must remain True"
        p_info_sc = h.rreturn("player get @s scrub")
        assert p_info_sc["data"]["overrides"]["player_position"] is False

        # Enable Scrub via Scrub tab toggle in UI!
        sc_x, sc_y = ui_pt("tab.scrub")
        h.raw_send(f"click-at {sc_x} {sc_y}")
        h.wait(2)
        tsx, tsy = ui_pt("opt.scrub_enabled")
        h.raw_send(f"click-at {tsx} {tsy}")
        h.wait(2)
        # Turn auto_save back off before gameplay rollback checks
        rbx, rby = ui_pt("tab.rollback")
        h.raw_send(f"click-at {rbx} {rby}")
        h.wait(2)
        asx, asy = ui_pt("opt.auto_save")
        h.raw_send(f"click-at {asx} {asy}")
        h.wait(2)

        h.raw_send("key esc tap")
        h.wait(2)
        assert h.screen().get("screen") == "playing"

        # 3. Verify persistence in <world>/rreturn/settings.json without touching config/rreturn.json
        cfg_path = _world_settings_path(h)
        cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
        po = cfg.get("playerOverrides", {})
        assert len(po) == 1, f"Expected 1 player override profile in world settings: {cfg}"
        only_prof = next(iter(po.values()))
        assert only_prof["rollback"].get("player_position") is False
        assert only_prof["scrub"].get("player_position") is False
        global_after = global_cfg_path.read_text(encoding="utf-8") if global_cfg_path.is_file() else None
        assert global_after == global_before, "Per-player world edits must not mutate config/rreturn.json"

        # 4. Functional Rollback test: Global player_position=true, @s=Deny -> blocks & inventory rewind, @s stays at new pos!
        h.tp(0.5, 65.0, 0.5)
        h.command("clear @s")
        h.command("give @s minecraft:apple 3")
        h.command("setblock 0 65 4 minecraft:air")
        h.wait(3)
        h.rreturn("set")
        h.wait(3)

        h.command("setblock 0 65 4 minecraft:emerald_block")
        h.command("clear @s")
        h.tp(22.5, 65.0, 22.5)
        h.wait(3)

        h.rreturn("load")
        h.wait(5)
        assert h.block(0, 65, 4).get("block") == "minecraft:air", "Blocks must rewind even when player_position is denied"
        assert h.inventory().get("counts", {}).get("minecraft:apple") == 3, "Inventory must rewind when only position is denied"
        pos_rb = h.state()["player"]["position"]
        assert abs(pos_rb[0] - 22.5) < 1.5 and abs(pos_rb[2] - 22.5) < 1.5, (
            f"Player position rewound despite per-player Deny override: {pos_rb}"
        )

        # 5. Functional Override Allow > Global False: set Global=false, @s=Allow -> @s MUST rewind!
        h.rreturn("policy set player_position false")
        h.rreturn("player set @s player_position true rollback")
        h.wait(2)
        h.rreturn("load")
        h.wait(5)
        pos_allow = h.state()["player"]["position"]
        assert abs(pos_allow[0] - 0.5) < 1.5 and abs(pos_allow[2] - 0.5) < 1.5, (
            f"Per-player Allow override did not beat Global=false: {pos_allow}"
        )

        # 6. Functional Scrub test with @s Scrub player_position = Deny: blocks rewind, @s stays!
        h.rreturn("policy reset")
        h.command("setblock 0 65 4 minecraft:air")
        h.tp(0.5, 65.0, 0.5)
        h.wait(20)
        h.command("setblock 0 65 4 minecraft:gold_block")
        h.tp(18.5, 65.0, 18.5)
        h.wait(4)
        assert h.block(0, 65, 4).get("block") == "minecraft:gold_block"

        h.rreturn("scrub rewind 0.9")
        h.wait(4)
        assert h.block(0, 65, 4).get("block") == "minecraft:air", "Scrub must rewind blocks"
        pos_sc = h.state()["player"]["position"]
        assert abs(pos_sc[0] - 18.5) < 1.5 and abs(pos_sc[2] - 18.5) < 1.5, (
            f"Scrub rewound position despite per-player scrub Deny override: {pos_sc}"
        )

        # 7. Reset per-player overrides back to Global via UI across both Rollback and Scrub scopes
        h.rreturn("menu players")
        h.wait(2)
        rbx, rby = ui_pt("p_scope.rollback")
        h.raw_send(f"click-at {rbx} {rby}")
        h.wait(2)
        rx, ry = ui_pt("player.reset_rules")
        h.raw_send(f"click-at {rx} {ry}")
        h.wait(2)
        sx, sy = ui_pt("p_scope.scrub")
        h.raw_send(f"click-at {sx} {sy}")
        h.wait(2)
        rx2, ry2 = ui_pt("player.reset_rules")
        h.raw_send(f"click-at {rx2} {ry2}")
        h.wait(2)
        h.raw_send("key esc tap")
        h.wait(2)

        cfg2 = json.loads(cfg_path.read_text(encoding="utf-8"))
        assert cfg2.get("playerOverrides", {}) == {}, f"Expected empty playerOverrides after reset: {cfg2}"
        assert cfg2.get("death_rollback") is True, cfg2

        # 8. Verify F8 keybinding is completely removed and no longer cycles anchor mode
        mode_before = h.rreturn("json").get("anchor_mode")
        h.raw_send("key f8 tap")
        h.wait(3)
        assert h.rreturn("json").get("anchor_mode") == mode_before, "F8 unexpectedly changed anchor_mode!"
    finally:
        if h.screen().get("screen") == "mod":
            h.raw_send("key esc tap")
            h.wait(2)
        h.rreturn("death on")
        h.rreturn("scrub off")
        h.rreturn("autosave adaptive")
        h.rreturn("autosave off")
        h.rreturn("policy reset")
        h.rreturn("scrub policy reset")
        h.rreturn("player reset all both")
        h.rreturn("anchor clear")
        h.command("setblock 0 65 4 minecraft:air")
        h.command("clear @s")
        h.tp(0.5, 65.0, 0.5)
        h.wait(2)


def test_118_mods_global_vs_world_settings(h):
    """Mods -> Config opens custom dark Global UI for new worlds, isolated from current-world K menu settings."""
    setup_clean_env(h)
    h.rreturn("death on")
    h.rreturn("scrub off")
    h.rreturn("policy reset")
    h.rreturn("scrub policy reset")
    h.rreturn("autosave adaptive")
    h.rreturn("autosave off")
    h.wait(2)

    orig_world = h.world
    global_cfg_path = _global_settings_path(h)
    world_a_cfg_path = _world_settings_path(h)
    backup_global = global_cfg_path.read_text(encoding="utf-8") if global_cfg_path.is_file() else None
    global_cfg_path.unlink(missing_ok=True)

    scratch_dir = Path(tempfile.gettempdir()) / "rreturn_e2e_screenshots"
    scratch_dir.mkdir(parents=True, exist_ok=True)

    def ui_bounds():
        res = h.rreturn("ui bounds")
        assert isinstance(res, dict) and "controls" in res, f"Expected ui bounds dict: {res}"
        return res

    def ui_pt(tag):
        controls = ui_bounds()["controls"]
        assert tag in controls, f"Missing UI hit control '{tag}' in {sorted(controls.keys())}"
        return controls[tag]

    def open_rreturn_from_mods_page():
        btns = h.screen().get("buttons", [])
        open_btn = next((b for b in btns if b.get("label") in ("Open Mods Folder", "Открыть папку модов")), None)
        back_btn = next((b for b in btns if b.get("label") in ("Back", "Назад")), None)
        if open_btn and back_btn:
            gui_w = int(round(open_btn["x"] + back_btn["x"] + back_btn["w"]))
        else:
            gui_w = int(round(ui_bounds()["window"][2]))
        cols_w = min(gui_w, 404)
        left = (gui_w - cols_w) // 2
        list_top = 32

        cfg_btn = None
        for row_idx in (2, 0, 1, 3, 4, 5):
            ry = list_top + 2 + row_idx * 28 + 14
            h.raw_send(f"click-at {left + 40} {ry}")
            time.sleep(0.15)
            btns_after = h.screen().get("buttons", [])
            cand = next((b for b in btns_after if b.get("label") in ("Config", "Настройки")), None)
            if cand is None and len(btns_after) >= 3:
                cand = btns_after[-3]
            if cand is not None and cand.get("enabled") is True:
                cfg_btn = cand
                break
        assert cfg_btn is not None, f"Could not select RReturn mod row with enabled Config: {h.screen().get('buttons', [])}"
        cx = int(round(cfg_btn["x"] + cfg_btn["w"] / 2.0))
        cy = int(round(cfg_btn["y"] + cfg_btn["h"] / 2.0))
        h.raw_send(f"click-at {cx} {cy}")
        time.sleep(0.2)

    try:
        # Prepare custom per-world setting in World A via K menu: enable auto_save only in World A
        h.raw_send("key k tap")
        time.sleep(0.2)
        assert h.screen().get("screen") == "mod", f"Expected mod screen after K, got: {h.screen()}"
        b_world = ui_bounds()
        assert b_world.get("mode") == "world", b_world
        assert "tab.players" in b_world["controls"], f"Expected Players tab in World menu: {b_world['controls'].keys()}"
        if b_world.get("active_tab") != "rollback":
            rx, ry = ui_pt("tab.rollback")
            h.raw_send(f"click-at {rx} {ry}")
            time.sleep(0.15)
        ax, ay = ui_pt("opt.auto_save")
        h.raw_send(f"click-at {ax} {ay}")
        time.sleep(0.2)
        h.raw_send(f"screenshot {scratch_dir / 'world_k_menu.png'}")
        time.sleep(0.15)
        h.raw_send("key esc tap")
        time.sleep(0.2)
        assert h.screen().get("screen") == "playing", f"Expected playing after closing K menu, got: {h.screen()}"
        assert world_a_cfg_path.is_file(), f"Missing per-world settings at {world_a_cfg_path}"
        world_a_before = json.loads(world_a_cfg_path.read_text(encoding="utf-8"))
        assert world_a_before["autosave"]["auto_save"] is True, world_a_before

        # 1. Pause/Options -> Mods -> Config opens custom dark RReturn Global menu (NOT vanilla Page::NativeMod)
        h.raw_send("options mods")
        time.sleep(0.2)
        assert h.screen().get("screen") == "options", f"Expected options screen, got: {h.screen()}"
        open_rreturn_from_mods_page()

        assert h.screen().get("screen") == "options", f"Expected Global modal overlay on options screen, got: {h.screen()}"
        bg = ui_bounds()
        assert bg.get("mode") == "global", f"Expected Global mode via Mods->Config: {bg}"
        assert bg.get("global_open") is True, bg
        assert bg.get("active_tab") == "rollback", bg
        for forbidden in ("tab.players", "jump.open_players"):
            assert forbidden not in bg["controls"], f"Players link '{forbidden}' must be hidden in Global mode"
        assert not any(k.startswith("jump.per_player.") for k in bg["controls"]), bg["controls"].keys()

        # Verify Scrub tab in Global mode also hides per-player links
        sx, sy = ui_pt("tab.scrub")
        h.raw_send(f"click-at {sx} {sy}")
        time.sleep(0.15)
        bg_scrub = ui_bounds()
        assert bg_scrub.get("active_tab") == "scrub", bg_scrub
        assert "jump.open_players" not in bg_scrub["controls"], bg_scrub["controls"].keys()
        assert "tab.players" not in bg_scrub["controls"], bg_scrub["controls"].keys()
        sc_x, sc_y = ui_pt("opt.scrub_enabled")
        h.raw_send(f"click-at {sc_x} {sc_y}")
        time.sleep(0.15)

        rb_x, rb_y = ui_pt("tab.rollback")
        h.raw_send(f"click-at {rb_x} {rb_y}")
        time.sleep(0.15)
        g_ax, g_ay = ui_pt("opt.auto_save")
        h.raw_send(f"click-at {g_ax} {g_ay}")
        time.sleep(0.15)
        dx, dy = ui_pt("opt.death_rollback")
        h.raw_send(f"click-at {dx} {dy}")
        time.sleep(0.15)
        bx, by = ui_pt("opt.blocks")
        h.raw_send(f"click-at {bx} {by}")
        time.sleep(0.2)
        rb_pts = dict(ui_bounds()["controls"])
        h.raw_send(f"screenshot {scratch_dir / 'global_mods_menu.png'}")
        time.sleep(0.15)

        # Global changes must persist to config/rreturn.json and leave active World A completely untouched
        assert global_cfg_path.is_file(), f"Expected global settings at {global_cfg_path}"
        g_cfg = json.loads(global_cfg_path.read_text(encoding="utf-8"))
        assert g_cfg["death_rollback"] is False, g_cfg
        assert g_cfg["checkpointPolicy"]["rollback"]["blocks"] is False, g_cfg
        assert g_cfg["scrub"]["enabled"] is True, g_cfg
        assert g_cfg["autosave"]["auto_save"] is False, g_cfg

        live_a = h.rreturn("json")
        assert live_a.get("death_rollback") is True, f"Global edit leaked into live world: {live_a}"
        assert live_a.get("scrub_enabled") is False, f"Global edit leaked into live world: {live_a}"
        assert live_a.get("auto_save") is True, f"Global edit leaked into live world: {live_a}"
        assert h.rreturn("policy get").get("blocks") is True, "Global edit leaked blocks policy into live world"
        assert json.loads(world_a_cfg_path.read_text(encoding="utf-8")) == world_a_before

        # Esc closes Global modal back to Mods screen, then Esc steps out via Main Options -> Paused -> Playing
        h.raw_send("key esc tap")
        time.sleep(0.2)
        bg_closed = ui_bounds()
        assert bg_closed.get("global_open") is False, bg_closed
        assert h.screen().get("screen") == "options", f"Expected options after closing modal, got: {h.screen()}"
        for _ in range(3):
            scr = h.screen().get("screen")
            if scr == "options":
                h.raw_send("key esc tap")
                time.sleep(0.2)
            elif scr == "paused":
                h.raw_send("resume")
                time.sleep(0.2)
            else:
                break
        assert h.screen().get("screen") == "playing", f"Expected playing after exiting Mods, got: {h.screen()}"

        # 2. Brand-new World B must inherit Global defaults on first load
        world_b = f"rreturn_e2e_cfg_b_{int(time.time())}"
        h.open_world(world_b, seed=54321)
        live_b = h.rreturn("json")
        assert live_b.get("death_rollback") is False, f"New world did not inherit death_rollback=false: {live_b}"
        assert live_b.get("scrub_enabled") is True, f"New world did not inherit scrub_enabled=true: {live_b}"
        assert live_b.get("auto_save") is False, f"New world inherited World A's auto_save=true: {live_b}"
        assert h.rreturn("policy get").get("blocks") is False, "New world did not inherit blocks=false"
        world_b_cfg_path = _world_settings_path(h)
        assert world_b_cfg_path.is_file(), f"Expected frozen world settings at {world_b_cfg_path}"

        # 3. Existing World A retains its own per-world settings when reopened
        h.open_world(orig_world, seed=12345)
        live_a2 = h.rreturn("json")
        assert live_a2.get("death_rollback") is True, f"Existing World A mutated by Global/World B: {live_a2}"
        assert live_a2.get("scrub_enabled") is False, f"Existing World A mutated by Global/World B: {live_a2}"
        assert live_a2.get("auto_save") is True, f"Existing World A lost its auto_save=true: {live_a2}"
        assert h.rreturn("policy get").get("blocks") is True, "Existing World A lost blocks=true"

        # 4. Title Screen -> Mods -> Config opens Global menu and closes cleanly via Close button
        assert h.exit_to_title(), "Failed to exit to title screen"
        title_btns = h.screen().get("buttons", [])
        mods_btn = next((b for b in title_btns if b.get("label") in ("Mods", "Mods...", "Моды")), None)
        assert mods_btn is not None, f"Missing Mods button on title screen: {title_btns}"
        h.raw_send(f"click-button {mods_btn['label']}")
        time.sleep(0.2)
        mods_btns = h.screen().get("buttons", [])
        assert any(b.get("label") in ("Config", "Настройки") for b in mods_btns), (
            f"Expected Mods screen after clicking Mods on title, got: {mods_btns}"
        )
        open_rreturn_from_mods_page()
        time.sleep(0.15)
        h.raw_send(f"screenshot {scratch_dir / 'global_title_mods_menu.png'}")
        time.sleep(0.15)

        # Reset toggled global flags back to defaults inside Global UI on Title screen using cached hit coords
        dx, dy = rb_pts["opt.death_rollback"]
        h.raw_send(f"click-at {dx} {dy}")
        time.sleep(0.15)
        bx, by = rb_pts["opt.blocks"]
        h.raw_send(f"click-at {bx} {by}")
        time.sleep(0.15)
        g_ax, g_ay = rb_pts["opt.auto_save"]
        h.raw_send(f"click-at {g_ax} {g_ay}")
        time.sleep(0.15)
        sx, sy = rb_pts["tab.scrub"]
        h.raw_send(f"click-at {sx} {sy}")
        time.sleep(0.15)
        sc_x, sc_y = bg_scrub["controls"]["opt.scrub_enabled"]
        h.raw_send(f"click-at {sc_x} {sc_y}")
        time.sleep(0.2)
        g_reset = json.loads(global_cfg_path.read_text(encoding="utf-8"))
        assert g_reset["death_rollback"] is True and g_reset["scrub"]["enabled"] is False, g_reset
        assert g_reset["autosave"]["auto_save"] is True, g_reset
        assert g_reset["checkpointPolicy"]["rollback"]["blocks"] is True, g_reset

        cx, cy = rb_pts["close"]
        h.raw_send(f"click-at {cx} {cy}")
        time.sleep(0.2)
        after_close_btns = h.screen().get("buttons", [])
        assert any(b.get("label") in ("Config", "Настройки") for b in after_close_btns), (
            f"Expected return to Mods screen after X close, got: {after_close_btns}"
        )
        for _ in range(3):
            btns_now = h.screen().get("buttons", [])
            back_btn = next((b for b in btns_now if b.get("label") in ("Back", "Назад", "Done", "Готово")), None)
            if back_btn is not None:
                h.raw_send(f"click-button {back_btn['label']}")
                time.sleep(0.2)
            else:
                break
        final_title_btns = h.screen().get("buttons", [])
        assert any(b.get("label") in ("Mods", "Mods...", "Моды", "Singleplayer", "Одиночная игра") for b in final_title_btns), (
            f"Expected return to main title menu after Esc from Mods, got: {final_title_btns}"
        )
    finally:
        for _ in range(3):
            try:
                scr = h.screen().get("screen")
                if scr in ("mod", "options"):
                    h.raw_send("key esc tap")
                    time.sleep(0.2)
                elif scr == "paused":
                    h.raw_send("resume")
                    time.sleep(0.2)
                else:
                    break
            except Exception:
                break
        if backup_global is not None:
            global_cfg_path.parent.mkdir(parents=True, exist_ok=True)
            global_cfg_path.write_text(backup_global, encoding="utf-8")
        else:
            global_cfg_path.unlink(missing_ok=True)
        if h.world != orig_world or h.screen().get("screen") != "playing":
            h.open_world(orig_world, seed=12345)
        h.rreturn("death on")
        h.rreturn("scrub off")
        h.rreturn("autosave adaptive")
        h.rreturn("autosave off")
        h.rreturn("policy reset")
        h.rreturn("scrub policy reset")
        h.wait(2)


def test_127_ui_polish_boss_default_players_heads_and_modes(h):
    """Fresh defaults turn boss autosave off; Global omits badge; Scrub omits prefix; Players shows 3 modes + head icons."""
    orig_world = h.world
    global_cfg_path = _global_settings_path(h)
    backup_global = global_cfg_path.read_text(encoding="utf-8") if global_cfg_path.is_file() else None
    global_cfg_path.unlink(missing_ok=True)
    fresh_world = f"{orig_world}_polish_{h.current_test_id}"
    h.open_world(fresh_world, seed=12345)

    def ui_bounds():
        res = h.rreturn("ui bounds")
        assert isinstance(res, dict) and "controls" in res, f"Expected ui bounds dict: {res}"
        return res

    def ui_pt(tag):
        controls = ui_bounds()["controls"]
        assert tag in controls, f"Missing UI hit control '{tag}' in {sorted(controls.keys())}"
        return controls[tag]

    try:
        # 1. save_on_boss_defeat defaults to False in a fresh world
        st_json = h.rreturn("json")
        assert st_json.get("save_on_boss_defeat") is False, (
            f"Expected save_on_boss_defeat to default to False: {st_json}"
        )

        # 2. Check localization polish in both lang files
        root = Path(__file__).resolve().parents[2]
        for lang_name in ("ru_ru.json", "en_us.json"):
            lang_data = json.loads((root / "assets/rreturn/lang" / lang_name).read_text(encoding="utf-8"))
            assert "rreturn.ui.global.badge" not in lang_data, f"Stale global.badge key in {lang_name}"
            for k in (
                "rreturn.ui.group.scrub_world",
                "rreturn.ui.group.scrub_entities",
                "rreturn.ui.group.scrub_player",
                "rreturn.ui.group.scrub_rules",
            ):
                val = lang_data[k]
                assert not val.upper().startswith(("СКРАБ:", "SCRUB:")), f"Prefix left in {lang_name} {k}={val}"
            ec_desc = lang_data["rreturn.ui.opt.player_ender_chest.desc"]
            assert "27" not in ec_desc and len(ec_desc) <= 32, (
                f"Expected concise ender chest desc without '27' in {lang_name}: {ec_desc}"
            )

        # 3. Players tab in K menu exposes 3 modes (Manual/Everyone/Rotating) & resolved player head icons
        h.raw_send("key k tap")
        h.wait(2)
        px, py = ui_pt("tab.players")
        h.raw_send(f"click-at {px} {py}")
        h.wait(4)
        b_pl = ui_bounds()
        ctrls = b_pl["controls"]
        for expected in ("anchor.mode.manual", "anchor.mode.everyone", "anchor.mode.rotating"):
            assert expected in ctrls, f"Missing 3-mode anchor button {expected}: {sorted(ctrls.keys())}"
        for removed in ("anchor.mode.fixed", "anchor.mode.multi"):
            assert removed not in ctrls, f"Removed anchor button {removed} still present: {sorted(ctrls.keys())}"

        heads = b_pl.get("player_heads", {})
        sel_uuid = b_pl.get("selected_player")
        assert sel_uuid and sel_uuid in heads and heads[sel_uuid], (
            f"Missing resolved head sprite for selected player {sel_uuid}: {heads}"
        )

        # Clicking each of the 3 mode buttons switches cleanly
        for seg, expected_mode in (
            ("anchor.mode.everyone", "everyone"),
            ("anchor.mode.rotating", "rotating"),
            ("anchor.mode.manual", "manual"),
        ):
            sx, sy = ui_pt(seg)
            h.raw_send(f"click-at {sx} {sy}")
            h.wait(2)
            assert h.rreturn("json").get("anchor_mode") == expected_mode, (
                f"Clicking {seg} did not set {expected_mode}: {h.rreturn('json')}"
            )

        # Legacy CLI aliases fixed/multi map cleanly to manual
        h.rreturn("anchor mode fixed")
        assert h.rreturn("json").get("anchor_mode") == "manual"
        h.rreturn("anchor mode multi")
        assert h.rreturn("json").get("anchor_mode") == "manual"
        h.raw_send("key esc tap")
        h.wait(2)

        # 4. Global modal has no global_badge and renders concise gray subtitle for new worlds
        h.raw_send("options mods")
        h.wait(2)
        btns = h.screen().get("buttons", [])
        open_btn = next((b for b in btns if b.get("label") in ("Open Mods Folder", "Открыть папку модов")), None)
        back_btn = next((b for b in btns if b.get("label") in ("Back", "Назад")), None)
        gui_w = int(round(open_btn["x"] + back_btn["x"] + back_btn["w"])) if open_btn and back_btn else 427
        left = (gui_w - min(gui_w, 404)) // 2
        for row_idx in (2, 0, 1, 3, 4, 5):
            h.raw_send(f"click-at {left + 40} {34 + row_idx * 28 + 14}")
            time.sleep(0.15)
            cand = next((b for b in h.screen().get("buttons", []) if b.get("label") in ("Config", "Настройки")), None)
            if cand and cand.get("enabled"):
                h.raw_send(f"click-at {int(round(cand['x'] + cand['w'] * 0.5))} {int(round(cand['y'] + cand['h'] * 0.5))}")
                time.sleep(0.2)
                break
        bg = ui_bounds()
        assert bg.get("mode") == "global" and bg.get("global_open") is True, bg
        assert bg.get("global_badge") is None, f"Global badge must be removed: {bg}"
        sub = bg.get("header_sub", "").lower()
        assert "нов" in sub or "new" in sub, f"Expected new-worlds subtitle in Global header: {bg}"
        h.raw_send("key esc tap")
        time.sleep(0.2)
    finally:
        for _ in range(3):
            try:
                scr = h.screen().get("screen")
                if scr in ("mod", "options"):
                    h.raw_send("key esc tap")
                    time.sleep(0.2)
                elif scr == "paused":
                    h.raw_send("resume")
                    time.sleep(0.2)
                else:
                    break
            except Exception:
                break
        if backup_global is not None:
            global_cfg_path.parent.mkdir(parents=True, exist_ok=True)
            global_cfg_path.write_text(backup_global, encoding="utf-8")
        else:
            global_cfg_path.unlink(missing_ok=True)
        h.open_world(orig_world, seed=12345)




