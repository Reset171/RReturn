"""Test 11: Timeline branching with named slots."""

import os
import shutil
import time
from tools.rreturn_harness import default_saves_dir
from . import setup_clean_env


def test_11_slots_branching(h):
    setup_clean_env(h)

    h.tp(50.5, 75.0, 50.5)
    h.wait(8)
    h.rreturn("save slot_alpha")
    h.wait(4)

    h.tp(0.5, 75.0, 0.5)
    h.wait(8)
    h.rreturn("save slot_beta")
    h.wait(4)

    slots = h.rreturn("slots")
    slot_names = [s["slot"] for s in slots.get("slots", [])]
    assert "slot_alpha" in slot_names, f"slot_alpha missing from slots: {slots}"
    assert "slot_beta" in slot_names, f"slot_beta missing from slots: {slots}"

    h.rreturn("load slot_alpha")
    h.wait(8)
    pos_a = h.state()["player"]["position"]
    assert abs(pos_a[0] - 50.5) < 1.5 and abs(pos_a[2] - 50.5) < 1.5, f"Did not rewind to alpha: {pos_a}"

    h.rreturn("load slot_beta")
    h.wait(8)
    pos_b = h.state()["player"]["position"]
    assert abs(pos_b[0] - 0.5) < 1.5 and abs(pos_b[2] - 0.5) < 1.5, f"Did not rewind to beta: {pos_b}"

    invalid_res = h.rreturn("slot save ../bad")
    assert "Invalid slot name" in str(invalid_res), f"Invalid slot name was not rejected: {invalid_res}"

    h.tp(25.5, 75.0, 25.5)
    h.wait(4)
    h.rreturn("save slot_io_guard")
    h.wait(4)
    slots_dir = default_saves_dir(h.instance) / h.world / "rreturn" / "slots"
    if slots_dir.is_dir() and hasattr(os, "geteuid") and os.geteuid() != 0:
        h.tp(60.5, 75.0, 60.5)
        h.wait(4)
        os.chmod(slots_dir, 0o555)
        try:
            io_res = h.rreturn("save slot_io_guard")
        finally:
            os.chmod(slots_dir, 0o755)
        err_text = str(io_res).lower()
        assert "failed" in err_text or "ошибка" in err_text or "i/o" in err_text, (
            f"Unwritable checkpoint save did not report I/O failure: {io_res}"
        )
        h.rreturn("load slot_io_guard")
        h.wait(6)
        pos_guard = h.state()["player"]["position"]
        assert abs(pos_guard[0] - 25.5) < 1.5 and abs(pos_guard[2] - 25.5) < 1.5, (
            f"Known-good checkpoint corrupted after I/O failure: {pos_guard}"
        )

    h.rreturn("slot delete slot_io_guard")
    h.rreturn("slot delete slot_alpha")
    h.rreturn("slot delete slot_beta")


def test_41_disk_checkpoint_reopen_and_rollback(h):
    setup_clean_env(h)
    h.tp(2400.5, 65.0, 2400.5)
    h.command("fill 2397 64 2397 2403 64 2403 minecraft:stone")
    h.command("fill 2397 65 2397 2403 70 2403 minecraft:air")
    h.command("kill @e[tag=rreturn_disk_golem]")
    h.command("kill @e[tag=rreturn_disk_drop]")
    h.command("setblock 2400 65 2402 minecraft:oak_log")
    h.command("setblock 2400 66 2402 minecraft:oak_log")
    h.command("summon minecraft:iron_golem 2402.5 65.0 2400.5 {NoAI:1b,Tags:['rreturn_disk_golem']}")
    h.wait(6)

    hp_before = h.data_get_entity("@e[tag=rreturn_disk_golem,limit=1]", "Health")

    h.rreturn("set")
    h.wait(6)

    h.command("setblock 2400 65 2402 minecraft:air")
    h.command("setblock 2400 66 2402 minecraft:air")
    h.command("summon minecraft:item 2400.5 65.0 2402.5 {Item:{id:'minecraft:oak_log',count:2},Tags:['rreturn_disk_drop']}")
    h.command("damage @e[tag=rreturn_disk_golem,limit=1] 10 minecraft:player_attack by @s")
    h.wait(4)

    b_mod = h.block(2400, 65, 2402)
    assert "air" in b_mod.get("block", ""), f"Oak log was not broken: {b_mod}"
    hp_hurt = h.data_get_entity("@e[tag=rreturn_disk_golem,limit=1]", "Health")
    assert hp_hurt != hp_before, f"Golem did not take damage: {hp_hurt}"

    h.rreturn("purge_memory")
    h.wait(4)

    h.rreturn("load")
    h.wait(10)

    b_restored_1 = h.block(2400, 65, 2402)
    b_restored_2 = h.block(2400, 66, 2402)
    assert b_restored_1.get("loaded") is True, f"Chunk was dropped/xray after disk rollback: {b_restored_1}"
    assert "oak_log" in b_restored_1.get("block", ""), f"Bottom oak log not restored from disk checkpoint: {b_restored_1}"
    assert "oak_log" in b_restored_2.get("block", ""), f"Top oak log not restored from disk checkpoint: {b_restored_2}"
    assert not h.has_entity("@e[tag=rreturn_disk_drop]"), "Dropped oak logs remained after disk rollback"

    hp_restored = h.data_get_entity("@e[tag=rreturn_disk_golem,limit=1]", "Health")
    assert hp_restored == hp_before, f"Golem health not restored from disk checkpoint: {hp_restored} vs {hp_before}"
    anger = h.data_get_entity("@e[tag=rreturn_disk_golem,limit=1]", "AngerTime")
    assert (
        "не найдены" in anger.lower()
        or "no entity" in anger.lower()
        or "found no" in anger.lower()
        or " 0" in anger
    ), f"Golem remained angry after disk rollback: {anger}"

    h.command("setblock 2401 65 2401 minecraft:torch")
    h.wait(3)
    assert "torch" in h.block(2401, 65, 2401).get("block", ""), "Torch was not placed for second rollback"

    h.rreturn("load")
    h.wait(6)
    assert "torch" not in h.block(2401, 65, 2401).get("block", ""), "Second rollback after disk hydration failed to remove torch"

    h.command("effect give @s minecraft:speed 120 1")
    h.command("worldborder center 2400 2400")
    h.command("worldborder set 500")
    h.wait(3)
    h.rreturn("set")
    h.wait(2)
    h.command("effect clear @s")
    h.command("worldborder set 2000")
    h.wait(2)
    h.rreturn("purge_memory")
    h.wait(2)
    h.rreturn("load")
    h.wait(6)
    eff_restored = h.data_get_entity("@s", "active_effects")
    assert "speed" in eff_restored, f"Player speed effect not preserved across disk reload: {eff_restored}"
    wb_res = h.command("worldborder get").get("chat", [])
    assert any("500" in line for line in wb_res), f"WorldBorder not preserved across disk reload: {wb_res}"
    h.command("effect clear @s")
    h.command("worldborder center 0 0")
    h.command("worldborder set 59999968")

    ground = h.block(2400, 64, 2400)
    assert ground.get("loaded") is True and "air" not in ground.get("block", ""), f"Ground became xray air: {ground}"

    h.command("kill @e[tag=rreturn_disk_golem]")
    h.command("fill 2397 65 2397 2403 70 2403 minecraft:air")
    h.tp(0.5, 65.0, 0.5)
    h.wait(4)


def test_44_disk_checkpoint_singleplayer_uuid_fallback(h):
    import json
    setup_clean_env(h)
    h.tp(500.5, 65.0, 500.5)
    h.wait(4)
    hp_saved = h.state()["player"]["health"]
    h.rreturn("set")
    h.wait(4)

    cp_meta = default_saves_dir(h.instance) / h.world / "rreturn" / "checkpoint" / "meta.json"
    assert cp_meta.is_file(), f"meta.json not found at {cp_meta}"

    with open(cp_meta, "r") as f:
        meta = json.load(f)

    # Заменяем UUID гостя на чужой UUID (например, имитируя запуск под другим профилем)
    dummy_uuid = "3bfd098ae4013b628300bbcb64541035"
    if meta.get("guests"):
        meta["guests"][0]["uuid"] = dummy_uuid
    with open(cp_meta, "w") as f:
        json.dump(meta, f)

    # Очищаем память, заставляя мод гидрировать снапшот с изменённым UUID с диска
    h.rreturn("purge_memory")
    h.wait(3)

    # Перемещаемся в другое место и наносим урон
    h.tp(600.5, 65.0, 600.5)
    h.command("damage @s 10 minecraft:generic")
    for _ in range(10):
        p_moved = h.state()["player"]
        if p_moved["health"] <= hp_saved - 5.0:
            break
        h.wait(2)

    assert abs(p_moved["position"][0] - 600.5) < 2.0, "Player did not move"
    assert p_moved["health"] < hp_saved, f"Player did not take damage: {p_moved['health']}"

    # Выполняем откат с диска
    h.rreturn("load")
    h.wait(8)

    p_after = h.state()["player"]
    pos_x = p_after["position"][0]
    pos_z = p_after["position"][2]
    assert abs(pos_x - 500.5) < 2.0 and abs(pos_z - 500.5) < 2.0, (
        f"Player position not restored from disk fallback: pos={p_after['position']}"
    )
    assert abs(p_after["health"] - hp_saved) < 1.0, (
        f"Player health not restored from disk fallback: health={p_after['health']} vs saved={hp_saved}"
    )
    h.tp(0.5, 65.0, 0.5)
    h.wait(4)


def test_124_disk_checkpoint_reopen_entity_type_client_sync(h):
    """F7 after world reopen must clear stale LAN entity tracker state so colliding IDs do not render old species on the client."""
    orig_world = h.world
    w_ent = f"rreturn_e2e_ent_kind_{int(time.time())}"

    try:
        h.open_world(w_ent, seed=12345, mode="creative")
        h.command("gamerule spawn_mobs false")
        h.command("difficulty hard")
        h.tp(0.5, 65.0, -3.0)
        h.command("fill -6 64 -6 10 64 10 minecraft:stone")
        h.command("fill -6 65 -6 10 72 10 minecraft:air")
        # Clear all ambient mobs, then reopen once so game.next_entity resets to 1 before F6
        h.command("kill @e[type=!player]")
        h.command("kill @e[type=!player]")
        h.wait(4)
        assert h.exit_to_title(timeout=30), "Failed initial clean reopen setup"
        h.open_world(w_ent, seed=12345, mode="creative")
        h.wait(6)

        def _client_nearby_mobs():
            return [
                e for e in h.entities(12).get("entities", [])
                if e.get("kind") == "mob"
            ]

        # Spawn 3 sheep + 1 pig -> low IDs starting right after player ID
        for i in range(3):
            h.command(f"summon minecraft:sheep {1.5 + i} 65.0 2.5 {{NoAI:1b,Tags:['t124_saved']}}")
        h.command("summon minecraft:pig 4.5 65.0 2.5 {NoAI:1b,Tags:['t124_saved']}")
        h.wait(8)

        saved_mobs = _client_nearby_mobs()
        near_types = [e.get("type") for e in saved_mobs]
        assert near_types.count("minecraft:sheep") == 3 and near_types.count("minecraft:pig") == 1, (
            f"Client did not receive initial sheep+pig: {saved_mobs}"
        )
        saved_ids = {int(e["id"]) for e in saved_mobs if e.get("type") in ("minecraft:sheep", "minecraft:pig")}
        max_saved_id = max(saved_ids)

        # F6 checkpoint + same-session F7 check
        h.rreturn("set")
        h.wait(4)
        h.rreturn("load")
        h.wait(6)
        same_types = [e.get("type") for e in _client_nearby_mobs()]
        assert same_types.count("minecraft:sheep") == 3 and same_types.count("minecraft:pig") == 1, (
            f"Same-session F7 broke sheep+pig on client: {same_types}"
        )

        # Replace sheep+pig with enough chickens + zombies before exiting
        # so reopening assigns colliding entity IDs (covering saved_ids) to chickens & zombies!
        h.command("kill @e[type=!player]")
        h.command("kill @e[type=!player]")
        h.wait(4)
        spawn_count = max_saved_id + 4
        for i in range(spawn_count):
            kind = "minecraft:chicken" if (i % 2 == 0) else "minecraft:zombie"
            x = 1.5 + (i % 6)
            z = 2.5 + ((i // 6) % 5)
            h.command(f"summon {kind} {x} 65.0 {z} {{NoAI:1b,Tags:['t124_other']}}")
        h.wait(8)

        rep_mobs = _client_nearby_mobs()
        rep_types = [e.get("type") for e in rep_mobs]
        assert rep_types.count("minecraft:chicken") >= 1 and rep_types.count("minecraft:zombie") >= 1, (
            f"Client did not receive replacement chicken+zombie: {rep_mobs}"
        )

        # Exit and reopen the world cleanly (fresh Server hub, replacement chickens+zombies take overlapping IDs)
        assert h.exit_to_title(timeout=30), "Failed to exit world to title"
        h.open_world(w_ent, seed=12345, mode="creative")
        h.wait(8)

        reopen_mobs = _client_nearby_mobs()
        reopen_ids = {int(e["id"]) for e in reopen_mobs}
        overlap_ids = saved_ids & reopen_ids
        assert overlap_ids == saved_ids, (
            f"Test setup failed to force full ID collision! saved_ids={sorted(saved_ids)}, reopen_ids={sorted(reopen_ids)}"
        )

        # Now press F7 after reopen -> disk hydration + client entity sync
        h.rreturn("load")
        h.wait(10)

        # Server must hold the restored sheep & pig
        assert h.has_entity("@e[type=minecraft:sheep,tag=t124_saved]"), "Server missing restored sheep"
        assert h.has_entity("@e[type=minecraft:pig,tag=t124_saved]"), "Server missing restored pig"

        # Client must visually show sheep + pig, NEVER stale chicken + zombie!
        client_mobs = _client_nearby_mobs()
        client_types = [e.get("type") for e in client_mobs]
        client_remote = h.status().get("server", {}).get("entities", {})
        assert client_types.count("minecraft:sheep") == 3 and client_types.count("minecraft:pig") == 1, (
            f"Client rendered wrong species after reopen F7! saved_mobs={saved_mobs}, reopen_mobs={reopen_mobs}, after_mobs={client_mobs}, remote={client_remote}"
        )
        assert client_types.count("minecraft:chicken") == 0 and client_types.count("minecraft:zombie") == 0, (
            f"Client kept chicken/zombie species after reopen F7! saved_mobs={saved_mobs}, reopen_mobs={reopen_mobs}, after_mobs={client_mobs}, remote={client_remote}"
        )

        # Also test direct disk hydration after purge_memory in the same running session
        h.command("kill @e[type=!player]")
        h.command("summon minecraft:chicken 1.5 65.0 2.5 {NoAI:1b,Tags:['t124_purge_chk']}")
        h.command("summon minecraft:zombie 4.5 65.0 2.5 {NoAI:1b,Tags:['t124_purge_chk']}")
        h.wait(6)
        h.rreturn("purge_memory")
        h.rreturn("load")
        h.wait(10)
        purge_types = [e.get("type") for e in _client_nearby_mobs()]
        assert purge_types.count("minecraft:sheep") == 3 and purge_types.count("minecraft:pig") == 1, (
            f"Client rendered wrong species after purge_memory + F7: {purge_types}"
        )
        assert purge_types.count("minecraft:chicken") == 0 and purge_types.count("minecraft:zombie") == 0, (
            f"Client kept chicken/zombie after purge_memory + F7: {purge_types}"
        )
    finally:
        h.exit_to_title(timeout=30)
        try:
            h.raw_send(f"world-delete {w_ent} --confirm")
        except Exception:
            pass
        world_dir = default_saves_dir(h.instance) / w_ent
        if world_dir.is_dir():
            shutil.rmtree(world_dir, ignore_errors=True)
        if orig_world and orig_world != w_ent:
            h.open_world(orig_world, seed=12345)
