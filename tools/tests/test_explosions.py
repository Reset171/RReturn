"""Tests 2 & 3: Massive TNT explosions and future ignition chain isolation."""

from . import setup_clean_env


def test_2_tnt_explosion_rollback(h):
    setup_clean_env(h)
    h.tp(200.5, 70, 200.5)
    h.command("fill 197 68 197 203 69 203 minecraft:stone")
    h.command("fill 197 70 197 203 74 203 minecraft:air")
    h.wait(3)

    h.rreturn("set")
    h.wait(2)

    h.command("fill 199 70 199 201 71 201 minecraft:tnt")
    h.command("summon tnt 200.5 70.0 200.5 {fuse:1s}")
    h.wait(8)

    crater = h.block(200, 69, 200)
    assert "stone" not in crater.get("block", ""), f"TNT did not destroy stone floor before rollback: {crater}"

    h.rreturn("load")
    h.wait(6)

    chk = h.block(200, 69, 200)
    assert "stone" in chk.get("block", ""), f"Crater remained after rollback, stone missing: {chk}"
    tnt_chk = h.block(200, 70, 200)
    assert "tnt" not in tnt_chk.get("block", ""), f"TNT blocks remained after rollback: {tnt_chk}"
    assert not h.has_entity("@e[type=tnt,distance=..16]"), "Primed TNT remained after rollback"


def test_3_tnt_future_chain_isolation(h):
    setup_clean_env(h)
    h.tp(300.5, 70, 300.5)
    h.command("fill 298 69 298 302 69 302 minecraft:bedrock")
    h.command("fill 299 70 299 301 70 301 minecraft:tnt")
    h.wait(3)

    h.rreturn("set")
    h.wait(2)

    h.command("summon tnt 300.5 71.0 300.5 {fuse:1s}")
    h.wait(8)

    destroyed = h.block(300, 70, 300)
    assert "tnt" not in destroyed.get("block", ""), f"TNT was not ignited in future timeline: {destroyed}"

    h.rreturn("load")
    h.wait(6)

    chk = h.block(300, 70, 300)
    assert "tnt" in chk.get("block", ""), f"TNT was destroyed by future explosion: {chk}"
    assert not h.has_entity("@e[type=tnt,distance=..16]"), "Future primed TNT leaked into past"
    h.command("fill 299 70 299 301 70 301 minecraft:air")
    h.wait(2)


def _capture_tnt_volume(h, server=False):
    prefix = "simulation blocks" if server else "blocks"
    s1 = h.raw_send(f"{prefix} -14 45 -14 24 64 24")
    s2 = h.raw_send(f"{prefix} -14 65 -14 24 85 24")
    return {
        "counts_low": s1.get("counts", {}),
        "counts_high": s2.get("counts", {}),
        "blocks_low": s1.get("blocks", []),
        "blocks_high": s2.get("blocks", []),
    }


def _step_until(h, target_gt, label):
    for _ in range(150):
        st = h.rreturn("status --json")
        if st.get("game_time", -1) >= target_gt:
            return st
        h.wait(2)
    raise AssertionError(f"{label}: failed to reach game_time {target_gt}: {h.rreturn('status --json')}")


def _state_sig(st):
    sig = {
        "level_rng": st.get("level_rng"),
        "gaussian": st.get("level_rng_gaussian"),
        "random_tick_value": st.get("random_tick_value"),
        "mobs": st.get("server_mobs"),
        "dropped": st.get("server_dropped"),
        "tnts": st.get("server_tnts"),
        "blocks_queued": st.get("server_blocks_queued"),
        "fluids_queued": st.get("server_fluids_queued"),
        "digest": st.get("server_entities_digest"),
    }
    assert sig["level_rng"] is not None and sig["digest"], f"Missing server state telemetry: {st}"
    return sig


def _run_tnt11_chain_and_capture(h, label):
    st_pre = h.rreturn("status --json")
    gt_pre = st_pre["game_time"]
    pre_sig = _state_sig(st_pre)
    h.command("setblock 0 72 -1 minecraft:redstone_block")

    h.command("tick step 85")
    st_85 = _step_until(h, gt_pre + 85, f"{label}_t+85")
    h.wait(3)
    mid85_blocks = _capture_tnt_volume(h)
    mid85_srv_blocks = _capture_tnt_volume(h, server=True)
    mid85_sig = _state_sig(st_85)

    h.command("tick step 20")
    st_105 = _step_until(h, gt_pre + 105, f"{label}_t+105")
    h.wait(3)
    mid105_blocks = _capture_tnt_volume(h)
    mid105_srv_blocks = _capture_tnt_volume(h, server=True)
    mid105_sig = _state_sig(st_105)

    h.command("tick unfreeze")
    waited = 105
    for _ in range(45):
        if not h.has_entity("@e[type=minecraft:tnt]"):
            break
        h.wait(10)
        waited += 10
    assert not h.has_entity("@e[type=minecraft:tnt]"), f"{label}: primed TNT did not finish exploding within {waited} ticks"
    h.wait(10)
    final_blocks = _capture_tnt_volume(h)
    final_srv_blocks = _capture_tnt_volume(h, server=True)
    assert final_blocks["counts_high"].get("minecraft:tnt", 0) == 0, (
        f"{label}: unexploded TNT remained: {final_blocks['counts_high']}"
    )
    return {
        "gt_pre": gt_pre,
        "pre_sig": pre_sig,
        "mid85_blocks": mid85_blocks,
        "mid85_srv_blocks": mid85_srv_blocks,
        "mid85_sig": mid85_sig,
        "mid105_blocks": mid105_blocks,
        "mid105_srv_blocks": mid105_srv_blocks,
        "mid105_sig": mid105_sig,
        "final_blocks": final_blocks,
        "final_srv_blocks": final_srv_blocks,
    }


def _assert_run_matches(label, run_x, baseline):
    assert run_x["pre_sig"] == baseline["pre_sig"], (
        f"{label} pre-ignition server state diverged! base={baseline['pre_sig']} got={run_x['pre_sig']}"
    )
    assert run_x["mid85_blocks"] == baseline["mid85_blocks"], (
        f"{label} +85t client blocks diverged! base={baseline['mid85_blocks']['counts_high']} got={run_x['mid85_blocks']['counts_high']}"
    )
    assert run_x["mid85_srv_blocks"] == baseline["mid85_srv_blocks"], (
        f"{label} +85t server blocks diverged! base={baseline['mid85_srv_blocks']['counts_high']} got={run_x['mid85_srv_blocks']['counts_high']}"
    )
    assert run_x["mid85_sig"] == baseline["mid85_sig"], (
        f"{label} +85t server entities/RNG diverged! base={baseline['mid85_sig']} got={run_x['mid85_sig']}"
    )
    assert run_x["mid105_blocks"] == baseline["mid105_blocks"], (
        f"{label} +105t client blocks diverged! base={baseline['mid105_blocks']['counts_high']} got={run_x['mid105_blocks']['counts_high']}"
    )
    assert run_x["mid105_srv_blocks"] == baseline["mid105_srv_blocks"], (
        f"{label} +105t server blocks diverged! base={baseline['mid105_srv_blocks']['counts_high']} got={run_x['mid105_srv_blocks']['counts_high']}"
    )
    assert run_x["mid105_sig"] == baseline["mid105_sig"], (
        f"{label} +105t server entities/RNG diverged! base={baseline['mid105_sig']} got={run_x['mid105_sig']}"
    )
    assert run_x["final_blocks"] == baseline["final_blocks"], (
        f"{label} final client crater diverged! base={baseline['final_blocks']['counts_low']} got={run_x['final_blocks']['counts_low']}"
    )
    assert run_x["final_srv_blocks"] == baseline["final_srv_blocks"], (
        f"{label} final server crater diverged! base={baseline['final_srv_blocks']['counts_low']} got={run_x['final_srv_blocks']['counts_low']}"
    )


def test_130_mass_tnt_fill_11x11_chain_determinism(h):
    """Full /fill ~ ~ ~ ~10 ~ ~10 minecraft:tnt chain explosion must be deterministic across F6/F7 (RAM & disk) and Scrub rewind."""
    setup_clean_env(h)
    try:
        h.command("gamemode creative")
        h.command("gamerule spawn_mobs false")
        h.command("kill @e[type=!player]")
        h.command("kill @e[type=!player]")
        h.command("fill -14 45 -14 24 59 24 minecraft:stone")
        h.command("fill -14 60 -14 24 71 24 minecraft:stone")
        h.command("fill -14 72 -14 24 92 24 minecraft:air")
        h.command("tp @s 0.5 72.0 0.5 0 0")
        h.ensure_loaded(10, 72, 10, timeout_ticks=120)
        h.command("kill @e[type=!player]")
        h.command("kill @e[type=!player]")
        h.command("fill ~ ~ ~ ~10 ~ ~10 minecraft:tnt")
        h.command("setblock -9 87 -9 minecraft:glass")
        h.command("tp @s -8.5 88.0 -8.5 -45 35")
        h.wait(4)

        init_vol = _capture_tnt_volume(h)
        assert init_vol["counts_high"].get("minecraft:tnt") == 121, (
            f"Expected 121 TNT blocks from /fill ~ ~ ~ ~10 ~ ~10 minecraft:tnt, got {init_vol['counts_high']}"
        )

        # Freeze at exact tick before ignition so every run starts from identical server tick & RNG stream,
        # then let the live explosion cascade finish unfrozen!
        h.command("tick freeze")
        h.wait(2)
        h.rreturn("save tnt11_base")
        h.wait(2)

        # Run 1: baseline forward explosion
        run1 = _run_tnt11_chain_and_capture(h, "run1_baseline")
        init_stone = init_vol["counts_low"].get("minecraft:stone", 0) + init_vol["counts_high"].get("minecraft:stone", 0)
        final_stone = run1["final_blocks"]["counts_low"].get("minecraft:stone", 0) + run1["final_blocks"]["counts_high"].get("minecraft:stone", 0)
        assert final_stone < init_stone - 200, (
            f"11x11 TNT sheet did not carve expected stone crater: before={init_stone}, after={final_stone}"
        )

        # Run 2: RAM F7 rollback -> full replay
        h.rreturn("load tnt11_base")
        h.wait(2)
        run2 = _run_tnt11_chain_and_capture(h, "run2_ram_load")
        _assert_run_matches("RAM load", run2, run1)

        # Run 3: Disk rollback after purge_memory -> full replay
        h.rreturn("purge_memory")
        h.wait(2)
        h.rreturn("load tnt11_base")
        h.wait(2)
        run3 = _run_tnt11_chain_and_capture(h, "run3_disk_load")
        _assert_run_matches("Disk load", run3, run1)

        # Run 4: Scrub mid-explosion back BEFORE ignition, commit, then ignite again!
        h.rreturn("load tnt11_base")
        h.wait(2)
        h.rreturn("scrub on")
        h.wait(4)
        h.command("tick unfreeze")
        h.wait(8)
        h.command("setblock 0 72 -1 minecraft:redstone_block")
        h.wait(105)

        h.rreturn("scrub start")
        h.wait(2)
        h.rreturn("scrub seek -30.0")
        h.wait(2)
        preview_vol = _capture_tnt_volume(h)
        assert preview_vol == init_vol, (
            f"Scrub preview before ignition did not match 121-TNT baseline! "
            f"clow1={init_vol['counts_low']} clowP={preview_vol['counts_low']} "
            f"chigh1={init_vol['counts_high']} chighP={preview_vol['counts_high']}"
        )
        h.rreturn("scrub commit")
        h.wait(2)
        h.rreturn("scrub off")
        h.command("tick freeze")
        h.wait(2)
        c4_0 = _capture_tnt_volume(h)
        srv4_0 = _capture_tnt_volume(h, server=True)
        assert c4_0 == init_vol and srv4_0 == init_vol, (
            f"Immediately after scrub commit+off, blocks did not match init_vol! "
            f"c4_0={c4_0['counts_high']} srv4_0={srv4_0['counts_high']} init={init_vol['counts_high']}"
        )

        run4 = _run_tnt11_chain_and_capture(h, "run4_after_scrub_commit")
        _assert_run_matches("Post-scrub-commit", run4, run1)
    finally:
        try:
            h.rreturn("scrub cancel")
        except Exception:
            pass
        h.rreturn("scrub off")
        h.command("tick unfreeze")
        h.command("kill @e[type=!player]")
        h.command("fill -14 45 -14 24 64 24 minecraft:stone")
        h.command("fill -14 65 -14 24 85 24 minecraft:air")
        h.command("gamerule spawn_mobs true")
        h.command("gamemode survival")
        h.wait(2)