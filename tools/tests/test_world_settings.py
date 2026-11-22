"""Tests 19-23: World Settings, Commands, Tick Rate, Weather, Biomes, Schedule."""

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

