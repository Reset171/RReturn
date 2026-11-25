"""Tests 19-23: World Settings, Commands, Tick Rate, Weather, Biomes, Schedule."""

import json
import tempfile
from pathlib import Path
from tools.rreturn_harness import default_saves_dir
from . import setup_clean_env


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

    # 4. Verify config/rreturn.json persisted independent rollback, scrub policies, and pinned player
    instance_dir = (default_saves_dir(h.instance) / h.world).parent.parent
    cfg_path = instance_dir / "config" / "rreturn.json"
    assert cfg_path.is_file(), f"Expected persistent config at {cfg_path}"
    saved_cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
    assert saved_cfg["checkpointPolicy"]["rollback"]["world"]["weather"] is False
    assert saved_cfg["checkpointPolicy"]["rollback"]["players"]["position"] is False
    assert saved_cfg["scrub"]["policy"]["world"]["weather"] is True
    assert saved_cfg["scrub"]["policy"]["players"]["position"] is False
    assert len(saved_cfg["anchor"]["pinned"]) >= 1, f"Expected pinned player in config: {saved_cfg['anchor']}"

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

        # 3. Verify persistence in config/rreturn.json
        instance_dir = (default_saves_dir(h.instance) / h.world).parent.parent
        cfg_path = instance_dir / "config" / "rreturn.json"
        cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
        po = cfg.get("playerOverrides", {})
        assert len(po) == 1, f"Expected 1 player override profile in config: {cfg}"
        only_prof = next(iter(po.values()))
        assert only_prof["rollback"].get("player_position") is False
        assert only_prof["scrub"].get("player_position") is False

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



