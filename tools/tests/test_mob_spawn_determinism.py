"""Tests 134-136: Complete mob spawning determinism (PRNG, UUIDs, custom spawners, disk hydration, Scrub)."""

import gzip
import json
import re
import time
from pathlib import Path

from tools.rreturn_harness import default_saves_dir
from . import setup_clean_env


def _parse_int_array(raw):
    m = re.search(r"\[I;\s*(-?\d+),\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\]", str(raw))
    if not m:
        return None
    return [int(m.group(i)) for i in range(1, 5)]


def _parse_pos_vec3(raw):
    m = re.search(
        r"\[\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)[dD]?\s*,\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)[dD]?\s*,\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)[dD]?\s*\]",
        str(raw),
    )
    if not m:
        return None
    return [round(float(m.group(i)), 4) for i in range(1, 4)]


def _parse_float_scalar(raw):
    m = re.search(r":\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)[fFdDbBsSlL]?\s*$", str(raw).strip())
    if not m:
        return None
    return round(float(m.group(1)), 5)


def _parse_int_scalar(raw):
    m = re.search(r":\s*(-?\d+)[bBsSlL]?\s*$", str(raw).strip())
    if not m:
        return None
    return int(m.group(1))


def _data_val(raw):
    s = str(raw).strip()
    if "has the following entity data:" in s:
        return s.split("has the following entity data:", 1)[1].strip()
    if "has the following block data:" in s:
        return s.split("has the following block data:", 1)[1].strip()
    return s


def _game_time(h):
    st = h.rreturn("status --json")
    gt = st.get("game_time")
    if gt is None:
        raise AssertionError(f"Missing game_time in rreturn status --json: {st}")
    return int(gt)


def _step_until(h, target_gt, label, timeout_polls=160):
    gt = _game_time(h)
    last_gt = gt
    last_progress_ts = time.time()
    for _ in range(timeout_polls):
        if gt >= target_gt:
            return gt
        h.wait(2)
        gt = _game_time(h)
        if gt > last_gt:
            last_gt = gt
            last_progress_ts = time.time()
        elif time.time() - last_progress_ts >= 30.0:
            st = h.rreturn("status --json")
            raise AssertionError(
                f"{label}: game_time stalled at {gt} (target={target_gt}, status={st})"
            )
    raise AssertionError(f"{label}: game_time did not reach {target_gt}, stuck at {gt}")


def _step_ticks(h, n, label):
    gt = _game_time(h)
    h.command(f"tick step {n}")
    return _step_until(h, gt + n, label, timeout_polls=max(120, n // 2 + 60))


def _sprint_ticks(h, n, label):
    gt = _game_time(h)
    h.command(f"tick sprint {n}")
    return _step_until(h, gt + n, label, timeout_polls=max(240, n // 4 + 240))


def _stabilize_chunks(h, max_polls=120):
    """Wait until async chunk streaming settles (not busy + 3 consecutive identical loaded_chunks counts)."""
    last = None
    stable = 0
    for _ in range(max_polls):
        st = h.rreturn("status --json")
        cur = st.get("loaded_chunks")
        busy = bool(st.get("streaming_busy", False))
        if not busy and cur is not None and cur > 0 and cur == last:
            stable += 1
            if stable >= 3:
                return cur
        else:
            stable = 0
            last = cur
        h.wait(2)
    return last


_TRACKED_MOB_TYPES = (
    "zombie_villager", "piglin", "zombie", "salmon", "skeleton", "spider",
    "slime", "magma_cube", "zombified_piglin", "pig", "creaking", "breeze",
    "villager", "pillager", "vindicator", "ravager", "evoker", "witch",
    "iron_golem", "creeper", "enderman", "drowned", "husk", "stray",
    "cave_spider", "bogged", "bat", "squid", "glow_squid", "cod",
    "chicken", "cow", "sheep", "wolf", "cat", "phantom", "wandering_trader",
    "trader_llama",
)


def _capture_zone_mob_fingerprints(h, cx, cy, cz, radius=60):
    """Capture strict deterministic server fingerprints for all living mobs around (cx,cy,cz).

    Note: numeric `entity.id` intentionally shifts by DISK_ID_MOD_STRIDE across disk reloads,
    so determinism asserts exact invariant identity: type, UUID, pos, baby, size, variant,
    left_handed, equipment, VillagerData, and key attributes.
    """
    h.command(f"tp @s {cx} {cy} {cz}")
    h.wait(4)
    # Tag every mob type purely server-side (avoids client packet lag during tick freeze).
    for mty in _TRACKED_MOB_TYPES:
        h.command(f"tag @e[type=minecraft:{mty},x={cx},y={cy},z={cz},distance=..{radius}] add rdet_t_{mty}")
    h.command(f"tag @e[type=!player,x={cx},y={cy},z={cz},distance=..{radius}] remove rdet_inspected")
    records = []
    for _ in range(160):
        sel = f"@e[type=!player,tag=!rdet_inspected,x={cx},y={cy},z={cz},distance=..{radius},sort=nearest,limit=1]"
        if not h.has_entity(sel):
            break
        raw_uuid = h.data_get_entity(sel, "UUID")
        if "has the following entity data:" not in raw_uuid:
            break
        raw_hp = h.data_get_entity(sel, "Health")
        # Mobs have float Health (...f); items have short Health (...s) and projectiles/orbs have none.
        if "has the following entity data:" not in raw_hp or not raw_hp.strip().endswith("f"):
            # Skip non-mob entities (items, orbs, lightning, projectiles)
            h.command(f"tag {sel} add rdet_inspected")
            continue
        uuid = _parse_int_array(raw_uuid)
        assert uuid is not None, f"Failed to parse UUID: {raw_uuid}"
        raw_pos = h.data_get_entity(sel, "Pos")
        pos = _parse_pos_vec3(raw_pos)
        assert pos is not None, f"Failed to parse Pos for UUID={uuid}: {raw_pos}"
        raw_tags = h.data_get_entity(sel, "Tags")
        mob_type = None
        for mty in _TRACKED_MOB_TYPES:
            if f"rdet_t_{mty}" in raw_tags:
                mob_type = f"minecraft:{mty}"
                break
        if not mob_type:
            # Fallback to client entity list by exact position
            for ce in h.entities(radius=radius).get("entities", []):
                if ce.get("kind") == "mob":
                    cpos = ce.get("pos") or [0.0, 0.0, 0.0]
                    if sum((float(cpos[i]) - pos[i]) ** 2 for i in range(3)) < 0.25:
                        mob_type = ce.get("type")
                        break
        assert mob_type, f"Could not resolve mob type at pos={pos} uuid={uuid} tags={raw_tags}"
        raw_eq = h.data_get_entity(sel, "equipment")
        eq = _data_val(raw_eq) if "has the following entity data:" in raw_eq else ""
        raw_lh = h.data_get_entity(sel, "LeftHanded")
        lh = _parse_int_scalar(raw_lh) if "has the following entity data:" in raw_lh else None
        raw_baby = h.data_get_entity(sel, "IsBaby")
        baby = _parse_int_scalar(raw_baby) if "has the following entity data:" in raw_baby else None
        raw_size = h.data_get_entity(sel, "Size")
        size = _parse_int_scalar(raw_size) if "has the following entity data:" in raw_size else None
        raw_vd = h.data_get_entity(sel, "VillagerData")
        vd = _data_val(raw_vd) if "has the following entity data:" in raw_vd else ""
        raw_attrs = h.data_get_entity(sel, "attributes")
        attrs = _data_val(raw_attrs) if "has the following entity data:" in raw_attrs else ""
        # Normalize float noise only beyond 1e-6 while preserving exact RNG-drawn bonuses
        raw_type_var = h.data_get_entity(sel, "type")
        type_var = _data_val(raw_type_var) if "has the following entity data:" in raw_type_var else ""
        h.command(f"tag {sel} add rdet_inspected")
        records.append({
            "type": mob_type,
            "uuid": uuid,
            "pos": pos,
            "equipment": eq,
            "left_handed": lh,
            "baby": baby,
            "size": size,
            "villager_data": vd,
            "type_var": type_var,
            "attributes": attrs,
        })
    h.command(f"tag @e[type=!player,x={cx},y={cy},z={cz},distance=..{radius}] remove rdet_inspected")
    for mty in _TRACKED_MOB_TYPES:
        h.command(f"tag @e[type=minecraft:{mty},x={cx},y={cy},z={cz},distance=..{radius}] remove rdet_t_{mty}")
    records.sort(key=lambda r: (r["type"], r["pos"][0], r["pos"][1], r["pos"][2], tuple(r["uuid"])))
    return records


def _assert_records_match(label, got, base):
    assert len(got) == len(base), (
        f"{label}: mob count diverged! base={len(base)} ({[r['type'] for r in base]}), "
        f"got={len(got)} ({[r['type'] for r in got]})"
    )
    for idx, (g, b) in enumerate(zip(got, base)):
        assert g == b, f"{label}: mob #{idx} ({b['type']}) diverged!\n  base={json.dumps(b)}\n  got ={json.dumps(g)}"


def _run_134_sequence(h, label):
    h.command("tp @s 2022.5 68.0 2005.5 0 30")
    _step_ticks(h, 2, f"{label}_s0")
    # Fire all 4 spawn-egg dispensers (3 bursts separated by redstone OFF->ON edges)
    for pulse in range(3):
        for dx in (2002, 2012, 2022, 2032):
            h.command(f"setblock {dx} 66 1997 minecraft:redstone_block")
        _step_ticks(h, 4, f"{label}_pulse{pulse}_on")
        for dx in (2002, 2012, 2022, 2032):
            h.command(f"setblock {dx} 66 1997 minecraft:air")
        _step_ticks(h, 4, f"{label}_pulse{pulse}_off")

    # Trigger lightning conversion (pig -> zombified_piglin) and slime split_dead_cube
    assert h.has_entity("@e[tag=test134_pig]"), f"{label}: test134_pig missing before lightning"
    assert h.has_entity("@e[tag=test134_slime]"), f"{label}: test134_slime missing before kill"
    h.command("execute at @e[tag=test134_pig,limit=1] run summon minecraft:lightning_bolt ~ ~ ~")
    h.command("kill @e[tag=test134_slime]")
    # Step enough ticks for Monster Spawner to fire too and entities to settle
    _step_ticks(h, 28, f"{label}_settle")

    records = _capture_zone_mob_fingerprints(h, 2022.5, 68.0, 2005.5, radius=48)
    counts = {}
    for r in records:
        counts[r["type"]] = counts.get(r["type"], 0) + 1
    assert counts.get("minecraft:zombie_villager", 0) >= 3, f"{label}: missing zombie_villagers: {counts}"
    assert counts.get("minecraft:piglin", 0) >= 3, f"{label}: missing piglins: {counts}"
    assert counts.get("minecraft:zombie", 0) >= 3, f"{label}: missing zombies: {counts}"
    assert counts.get("minecraft:salmon", 0) >= 3, f"{label}: missing salmon: {counts}"
    assert counts.get("minecraft:zombified_piglin", 0) >= 1, f"{label}: lightning convert_to zombified_piglin missing: {counts}"
    assert counts.get("minecraft:slime", 0) >= 2, f"{label}: slime split children missing: {counts}"
    total_spawner_pool = (
        counts.get("minecraft:skeleton", 0)
        + counts.get("minecraft:spider", 0)
        + max(0, counts.get("minecraft:zombie", 0) - 3)
        + max(0, counts.get("minecraft:piglin", 0) - 3)
        + max(0, counts.get("minecraft:zombie_villager", 0) - 3)
    )
    assert total_spawner_pool >= 1, f"{label}: Monster Spawner did not spawn any mobs: {counts}"
    return records


def test_134_mob_creation_rng_and_uuid_determinism(h):
    """Mob creation (living_random/Mob::new/constructor_draws/finalize/convert_to/split/spawner) must be deterministic across RAM, Disk, and Scrub."""
    setup_clean_env(h)
    try:
        h.command("gamemode creative")
        h.command("difficulty hard")
        h.command("gamerule spawn_mobs false")
        h.tp(2022.5, 68.0, 2005.5)
        h.ensure_loaded(2000, 64, 1995, timeout_ticks=120)
        h.ensure_loaded(2040, 64, 2015, timeout_ticks=120)
        h.command("kill @e[type=!player,distance=..80]")

        # Arena base floor & clearing
        h.command("fill 1995 63 1994 2046 64 2018 minecraft:stone")
        h.command("fill 1995 65 1994 2046 76 2018 minecraft:air")

        # 4 spawn-egg pens (each completely enclosed with glass/stone ceiling so nothing burns/wanders out)
        eggs = [
            (2002, "minecraft:zombie_villager_spawn_egg"),
            (2012, "minecraft:piglin_spawn_egg"),
            (2022, "minecraft:zombie_spawn_egg"),
            (2032, "minecraft:salmon_spawn_egg"),
        ]
        for bx, egg in eggs:
            h.command(f"fill {bx-3} 65 1998 {bx+3} 69 2004 minecraft:stone")
            h.command(f"fill {bx-2} 65 1999 {bx+2} 68 2003 minecraft:air")
            if "salmon" in egg:
                h.command(f"fill {bx-2} 65 1999 {bx+2} 68 2003 minecraft:water")
            h.command(f"setblock {bx} 66 1998 minecraft:dispenser[facing=south]")
            h.command(f"item replace block {bx} 66 1998 container.0 with {egg} 8")

        # Monster Spawner room & pig/slime pens (built empty before chunk settle; armed while frozen)
        h.command("fill 2018 65 2007 2026 70 2014 minecraft:stone")
        h.command("fill 2019 65 2008 2025 69 2013 minecraft:air")
        h.command("fill 1997 65 2007 2005 68 2014 minecraft:stone")
        h.command("fill 1998 65 2008 2004 68 2013 minecraft:air")

        # Player survival booth
        h.command("fill 2020 67 2004 2024 70 2007 minecraft:glass")
        h.command("fill 2021 68 2005 2023 69 2006 minecraft:air")
        h.command("tp @s 2022.5 68.0 2005.5 0 30")
        h.command("gamemode survival")
        _stabilize_chunks(h)

        h.command("tick freeze")
        h.wait(2)
        h.command("kill @e[type=!player,distance=..80]")
        # Arm spawners + test entities strictly while frozen so no pre-save GoalSelector drift occurs
        h.command(
            'setblock 2022 65 2010 minecraft:spawner{Delay:4s,MinSpawnDelay:8s,MaxSpawnDelay:8s,SpawnCount:2s,MaxNearbyEntities:12s,RequiredPlayerRange:24s,SpawnPotentials:[{weight:3,data:{entity:{id:"minecraft:zombie"},custom_spawn_rules:{block_light_limit:{min_inclusive:0,max_inclusive:15},sky_light_limit:{min_inclusive:0,max_inclusive:15}}}},{weight:3,data:{entity:{id:"minecraft:skeleton"},custom_spawn_rules:{block_light_limit:{min_inclusive:0,max_inclusive:15},sky_light_limit:{min_inclusive:0,max_inclusive:15}}}},{weight:2,data:{entity:{id:"minecraft:piglin"},custom_spawn_rules:{block_light_limit:{min_inclusive:0,max_inclusive:15},sky_light_limit:{min_inclusive:0,max_inclusive:15}}}},{weight:2,data:{entity:{id:"minecraft:zombie_villager"},custom_spawn_rules:{block_light_limit:{min_inclusive:0,max_inclusive:15},sky_light_limit:{min_inclusive:0,max_inclusive:15}}}}]}'
        )
        # Fixed-Pos SpawnData spawner exercises Planned::Loaded -> load_mob alongside multi-spawner BE ticker order
        h.command(
            'setblock 2020 65 2012 minecraft:spawner{Delay:2s,MinSpawnDelay:24s,MaxSpawnDelay:24s,SpawnCount:1s,MaxNearbyEntities:12s,RequiredPlayerRange:24s,SpawnData:{entity:{id:"minecraft:skeleton",Pos:[2021.5d,66.0d,2011.5d],PersistenceRequired:1b},custom_spawn_rules:{block_light_limit:{min_inclusive:0,max_inclusive:15},sky_light_limit:{min_inclusive:0,max_inclusive:15}}}}'
        )
        h.command("summon minecraft:pig 2000.5 65.0 2010.5 {NoAI:1b,Tags:['test134_pig']}")
        h.command("summon minecraft:slime 2003.5 65.0 2010.5 {NoAI:1b,Size:2,Tags:['test134_slime']}")
        h.rreturn("save mobs134_base")
        h.wait(2)

        # Run 1: Baseline
        run1 = _run_134_sequence(h, "run1_baseline")

        # Run 2: RAM rollback -> full replay
        h.rreturn("load mobs134_base")
        h.wait(2)
        run2 = _run_134_sequence(h, "run2_ram_load")
        _assert_records_match("RAM F7 load", run2, run1)

        # Run 3: Disk rollback (after purge_memory -> shifts entity IDs by DISK_ID_MOD_STRIDE!)
        h.rreturn("purge_memory")
        h.wait(2)
        h.rreturn("load mobs134_base")
        h.wait(2)
        run3 = _run_134_sequence(h, "run3_disk_load")
        _assert_records_match("Disk load (purge_memory)", run3, run1)

        # Run 4: Scrub forward mid-sequence -> rewind back before spawn -> commit -> replay
        h.rreturn("load mobs134_base")
        h.wait(2)
        h.rreturn("scrub on")
        h.wait(3)
        gt0 = _game_time(h)
        _run_134_sequence(h, "run4_pre_scrub")
        gt1 = _game_time(h)
        delta = gt1 - gt0
        assert delta > 10, f"Unexpected scrub delta {delta}"

        h.rreturn("scrub start")
        h.wait(2)
        h.rreturn(f"scrub rewind_ticks {delta}")
        h.wait(2)
        h.rreturn("scrub commit")
        h.wait(2)
        h.rreturn("scrub off")
        h.wait(2)
        assert not h.has_entity("@e[type=piglin,distance=..48]"), "Future piglin leaked after scrub commit"
        run4 = _run_134_sequence(h, "run4_after_scrub_commit")
        _assert_records_match("Post-scrub commit", run4, run1)
    finally:
        try:
            h.rreturn("scrub cancel")
        except Exception:
            pass
        h.rreturn("scrub off")
        h.command("tick unfreeze")
        h.command("gamerule spawn_mobs true")
        h.command("kill @e[type=!player,distance=..100]")
        h.command("fill 1995 65 1994 2046 76 2018 minecraft:air")
        h.command("gamemode survival")
        h.wait(2)


def _run_135_sequence(h, label):
    # Power the ceiling circuit while frozen: CB at (2316,72,2296) gives bad_omen while player is outside
    # TrialSpawner range (so bad_omen -> raid_omen), then repeater-delayed CB teleports into TrialSpawner range.
    h.command("setblock 2316 72 2295 minecraft:redstone_block")
    _step_ticks(h, 12, f"{label}_cb_tp12")
    h.command("fill 2315 72 2294 2320 72 2298 air")
    # 600t raid_omen countdown + 300t Raid Wave 1 spawn delay + margin; Trial Spawner + Creaking also run here.
    _sprint_ticks(h, 980, f"{label}_raid_and_trial")

    # Inspect Trial Spawner block NBT directly (contains current_mobs UUID list & spawn_data)
    trial_current = h.data_get_block(2302, 65, 2300, "current_mobs")
    assert "has the following block data:" in trial_current, (
        f"{label}: Trial spawner has no current_mobs (did not spawn?): {trial_current}"
    )

    records = _capture_zone_mob_fingerprints(h, 2315.5, 65.0, 2300.5, radius=96)
    counts = {}
    for r in records:
        counts[r["type"]] = counts.get(r["type"], 0) + 1
    raiders = (
        counts.get("minecraft:pillager", 0)
        + counts.get("minecraft:vindicator", 0)
        + counts.get("minecraft:ravager", 0)
        + counts.get("minecraft:witch", 0)
        + counts.get("minecraft:evoker", 0)
    )
    assert raiders >= 1, (
        f"{label}: Raid Wave 1 raiders did not spawn (effects={h.data_get_entity('@s', 'active_effects')}, "
        f"villager_bed={h.data_get_entity('@e[tag=test135_villager,limit=1]', 'Brain')}): {counts}"
    )
    return {"trial_current": _data_val(trial_current), "records": records}


def test_135_trial_spawner_creaking_and_raid_determinism(h):
    """Trial Spawner UUIDs/hash_order, Creaking Heart UUID+brain, and Raid Wave 1 RNG must be deterministic."""
    setup_clean_env(h)
    try:
        h.command("gamemode creative")
        h.command("difficulty hard")
        h.command("time set 18000")
        h.command("gamerule spawn_mobs true")
        h.command("gamerule command_blocks_work true")
        h.tp(2307.5, 65.0, 2300.5)
        h.ensure_loaded(2270, 64, 2240, timeout_ticks=120)
        h.ensure_loaded(2390, 64, 2360, timeout_ticks=120)
        h.command("kill @e[type=!player,distance=..120]")

        # Flat stone floor covering full 64-block Raid Wave 1 spawn radius around village bed (2324, 65, 2300)
        h.command("fill 2268 64 2236 2396 64 2364 minecraft:stone")
        h.command("fill 2268 65 2236 2396 80 2364 minecraft:air")

        # 1. Trial Spawner enclosed arena around (2302, 65, 2300)
        h.command("fill 2299 65 2295 2309 70 2305 minecraft:stone")
        h.command("fill 2300 65 2296 2308 69 2304 minecraft:air")
        # Glass observer booth at (2308, 65, 2300): VisualShape::Empty lets TrialSpawner line-of-sight
        # detect player (< 10 range, > 4 spawn range), while solid Collider blocks WindCharge knockback during tick sprint.
        h.command("fill 2307 65 2299 2309 68 2301 minecraft:glass")
        h.command("fill 2308 65 2300 2308 67 2300 minecraft:air")
        # Infinite duration so real-time 20Hz freeze ticking cannot alter effect duration
        h.command("effect give @s minecraft:resistance infinite 4 true")

        # 2. Creaking Heart enclosed dark room logs around (2315, 65, 2300) & ceiling trigger circuit
        h.command("fill 2310 65 2295 2320 70 2305 minecraft:stone")
        h.command("fill 2311 65 2296 2319 69 2304 minecraft:air")
        h.command("setblock 2315 65 2300 minecraft:pale_oak_log[axis=y]")
        h.command("setblock 2315 67 2300 minecraft:pale_oak_log[axis=y]")
        h.command("fill 2315 71 2294 2320 71 2298 minecraft:stone")
        h.command(
            'setblock 2316 72 2296 minecraft:command_block{auto:0b,Command:"effect give @p minecraft:bad_omen 5 0 true"}'
        )
        h.command("setblock 2317 72 2295 minecraft:redstone_wire")
        h.command("setblock 2318 72 2295 minecraft:repeater[facing=west,delay=2]")
        h.command(
            'setblock 2319 72 2295 minecraft:command_block{auto:0b,Command:"tp @p 2308.5 65.0 2300.5 0 0"}'
        )

        # 3. Village bed + caged villager at (2324, 65, 2300) in section (145, 4, 143) -> 1 section from (2308.5, 65, 2300.5)
        h.command("fill 2322 65 2298 2326 68 2302 minecraft:glass")
        h.command("fill 2323 65 2299 2325 67 2301 minecraft:air")
        h.command("setblock 2324 65 2300 minecraft:red_bed[facing=north,part=foot]")
        h.command("setblock 2324 65 2299 minecraft:red_bed[facing=north,part=head]")
        h.command("summon minecraft:villager 2323.5 65.0 2300.5 {NoAI:0b,PersistenceRequired:1b,Tags:['test135_villager']}")

        # Let villager claim bed before checkpoint, then stand at (2316.5, 65.0, 2307.5) outside Trial Spawner range (10)
        h.tp(2323.5, 65.0, 2300.5)
        _stabilize_chunks(h)
        h.wait(45)
        h.tp(2316.5, 65.0, 2307.5)
        h.command("gamemode survival")
        _stabilize_chunks(h)
        h.command("kill @e[type=!player,tag=!test135_villager,distance=..120]")

        h.command("tick freeze")
        h.wait(2)
        h.command(
            'setblock 2302 65 2300 minecraft:trial_spawner[trial_spawner_state=waiting_for_players]{required_player_range:10,normal_config:{simultaneous_mobs:3.0f,total_mobs:4.0f,ticks_between_spawn:5,spawn_potentials:[{weight:2,data:{entity:{id:"minecraft:zombie"}}},{weight:2,data:{entity:{id:"minecraft:skeleton"}}},{weight:1,data:{entity:{id:"minecraft:breeze"}}}]}}'
        )
        h.command("setblock 2315 66 2300 minecraft:creaking_heart[axis=y,creaking_heart_state=awake,natural=false]")
        h.rreturn("save mobs135_base")
        h.wait(2)

        # Run 1: Baseline
        run1 = _run_135_sequence(h, "run1_baseline")

        # Run 2: RAM F7 load
        h.rreturn("load mobs135_base")
        h.wait(2)
        run2 = _run_135_sequence(h, "run2_ram_load")
        assert run2["trial_current"] == run1["trial_current"], (
            f"RAM load Trial Spawner current_mobs diverged! base={run1['trial_current']} got={run2['trial_current']}"
        )
        _assert_records_match("RAM F7 load", run2["records"], run1["records"])

        # Run 3: Disk load (purge_memory -> F7)
        h.rreturn("purge_memory")
        h.wait(2)
        h.rreturn("load mobs135_base")
        h.wait(2)
        run3 = _run_135_sequence(h, "run3_disk_load")
        assert run3["trial_current"] == run1["trial_current"], (
            f"Disk load Trial Spawner current_mobs diverged! base={run1['trial_current']} got={run3['trial_current']}"
        )
        _assert_records_match("Disk load (purge_memory)", run3["records"], run1["records"])

        # Run 4: Scrub forward -> rewind before raid/spawns -> commit -> replay
        h.rreturn("load mobs135_base")
        h.wait(2)
        h.rreturn("scrub on")
        h.wait(3)
        gt0 = _game_time(h)
        _run_135_sequence(h, "run4_pre_scrub")
        gt1 = _game_time(h)
        delta = gt1 - gt0

        h.rreturn("scrub start")
        h.wait(2)
        h.rreturn(f"scrub rewind_ticks {delta}")
        h.wait(2)
        h.rreturn("scrub commit")
        h.wait(2)
        h.rreturn("scrub off")
        h.wait(2)
        run4 = _run_135_sequence(h, "run4_after_scrub_commit")
        assert run4["trial_current"] == run1["trial_current"], (
            f"Scrub commit Trial Spawner current_mobs diverged! base={run1['trial_current']} got={run4['trial_current']}"
        )
        _assert_records_match("Post-scrub commit", run4["records"], run1["records"])
    finally:
        try:
            h.rreturn("scrub cancel")
        except Exception:
            pass
        h.rreturn("scrub off")
        h.command("tick unfreeze")
        h.command("effect clear @s")
        h.command("time set day")
        h.command("gamerule spawn_mobs true")
        h.command("kill @e[type=!player,distance=..140]")
        h.command("fill 2268 65 2236 2396 80 2364 minecraft:air")
        h.command("gamemode survival")
        h.wait(2)


def _read_trader_spawn_delay(world_dir):
    dat_path = Path(world_dir) / "data" / "minecraft" / "wandering_trader.dat"
    if not dat_path.exists():
        return None
    raw = dat_path.read_bytes()
    # Quick big-endian Int search for "spawn_delay" NBT tag inside uncompressed/gzip NBT
    if raw[:2] == b"\x1f\x8b":
        raw = gzip.decompress(raw)
    needle = b"\x03\x00\x0bspawn_delay"
    idx = raw.find(needle)
    if idx < 0:
        return None
    val_off = idx + len(needle)
    return int.from_bytes(raw[val_off:val_off + 4], "big", signed=True)


def test_136_custom_spawner_schedules_rollback_determinism(h):
    """WanderingTrader tick_delay and PatrolSpawner next_tick must restore accurately across RAM, Disk, and Scrub."""
    setup_clean_env(h)
    try:
        world_dir = default_saves_dir(h.instance) / h.world

        h.command("gamemode creative")
        h.command("difficulty normal")
        # Enable patrols (>120,000 day_time) and spawn_mobs
        h.command("time set 125000")
        h.command("gamerule spawn_mobs true")
        h.tp(2600.5, 68.0, 2600.5)
        h.ensure_loaded(2560, 64, 2560, timeout_ticks=120)
        h.ensure_loaded(2640, 64, 2640, timeout_ticks=120)
        h.command("kill @e[type=!player,distance=..120]")
        h.command("fill 2560 64 2560 2640 64 2640 minecraft:stone")
        h.command("fill 2560 65 2560 2640 76 2640 minecraft:air")
        # Shield player inside a sealed glass safety booth + infinite resistance so night mobs cannot trigger death_rollback mid-sprint
        h.command("fill 2599 65 2599 2601 68 2601 minecraft:glass")
        h.command("fill 2600 66 2600 2600 67 2600 minecraft:air")
        h.command("effect give @s minecraft:resistance infinite 4 true")
        h.command("tp @s 2600.5 66.0 2600.5 0 0")
        h.command("gamemode survival")
        _stabilize_chunks(h)

        h.command("tick freeze")
        h.wait(2)
        h.rreturn("save sched136_init")
        h.save()
        h.wait(4)
        sd_start = _read_trader_spawn_delay(world_dir)
        sd_base = None
        for idx in range(13):
            _sprint_ticks(h, 100, f"trader_align_{idx}")
            h.command("tick freeze")
            h.wait(2)
            h.save()
            h.wait(3)
            cur_sd = _read_trader_spawn_delay(world_dir)
            if cur_sd is not None and cur_sd != sd_start:
                sd_base = cur_sd
                break
        assert sd_base is not None, (
            f"Wandering trader did not cross due countdown within 1300t (start={sd_start})"
        )
        _sprint_ticks(h, 600, "trader_mid_600")
        h.command("tick freeze")
        h.wait(2)
        h.save()
        h.wait(3)
        sd_mid = _read_trader_spawn_delay(world_dir)
        assert sd_mid == sd_base, (
            f"Expected wandering_trader spawn_delay to stay {sd_base} before due, got {sd_mid}"
        )
        h.rreturn("save sched136_trader_mid")
        h.wait(2)

        def _check_trader_due(label):
            _sprint_ticks(h, 700, f"{label}_sprint700")
            h.command("tick freeze")
            h.wait(2)
            h.save()
            h.wait(4)
            sd = _read_trader_spawn_delay(world_dir)
            expected = sd_base - 1200 if sd_base >= 1200 else 24000
            assert sd == expected, (
                f"{label}: wandering_trader tick_delay did not preserve countdown across rollback! "
                f"Expected spawn_delay={expected} after +700t from base={sd_base}, got {sd}"
            )

        _check_trader_due("run1_trader")

        h.rreturn("load sched136_trader_mid")
        h.wait(2)
        _check_trader_due("ram_load_trader")

        h.rreturn("purge_memory")
        h.wait(2)
        h.rreturn("load sched136_trader_mid")
        h.wait(2)
        _check_trader_due("disk_load_trader")

        # 2. PatrolSpawner next_tick test:
        # Sprint across the patrol interval (~12000-13200t): save at tick +11800 (Patrol next_tick ~200..1400),
        # advance +1600t (so Patrol fires & resets live next_tick to ~12000), then rollback to +11800 and verify
        # exact same RNG stream & patrol triggers during the replayed +1600t window via a final weighted mob draw!
        h.rreturn("load sched136_init")
        h.wait(2)
        h.command("time set 125000")
        h.command("gamerule spawn_mobs true")
        # Sprint 11800 ticks using /tick sprint and wait for sprint completion
        gt_before_sprint = _game_time(h)
        h.command("tick sprint 11800")
        _step_until(h, gt_before_sprint + 11800, "patrol_sprint_11800", timeout_polls=1200)
        h.command("tick freeze")
        h.wait(2)
        # Dispenser test probes world.random after replay
        h.command("setblock 2603 65 2602 minecraft:dispenser[facing=north]")
        for slot in range(6):
            h.command(f"item replace block 2603 65 2602 container.{slot} with minecraft:zombie_villager_spawn_egg 4")
        h.rreturn("save sched136_patrol_near")
        h.wait(2)

        def _run_patrol_window(label):
            gt0 = _game_time(h)
            h.command("tick sprint 1600")
            _step_until(h, gt0 + 1600, f"{label}_sprint_1600", timeout_polls=400)
            h.command("tick freeze")
            h.wait(2)
            # Fire 3 dispenser shots -> reads world.random slot pick + entity spawn RNG
            for pulse in range(3):
                h.command("setblock 2603 65 2603 minecraft:redstone_block")
                _step_ticks(h, 4, f"{label}_p{pulse}_on")
                h.command("setblock 2603 65 2603 minecraft:air")
                _step_ticks(h, 4, f"{label}_p{pulse}_off")
            return _capture_zone_mob_fingerprints(h, 2600.5, 65.0, 2600.5, radius=64)

        patrol_run1 = _run_patrol_window("run1_patrol")

        # After patrol_run1, live PatrolSpawner has fired & reset next_tick back to ~12000!
        # Rolling back to sched136_patrol_near MUST restore patrol.next_tick back near ~200..1400.
        h.rreturn("load sched136_patrol_near")
        h.wait(2)
        patrol_ram = _run_patrol_window("ram_load_patrol")
        _assert_records_match("RAM load PatrolSpawner schedule", patrol_ram, patrol_run1)

        h.rreturn("purge_memory")
        h.wait(2)
        h.rreturn("load sched136_patrol_near")
        h.wait(2)
        patrol_disk = _run_patrol_window("disk_load_patrol")
        _assert_records_match("Disk load PatrolSpawner schedule", patrol_disk, patrol_run1)

        # Scrub rewind across Patrol & Trader boundaries
        h.rreturn("load sched136_patrol_near")
        h.wait(2)
        h.rreturn("scrub on")
        h.wait(3)
        gt_s0 = _game_time(h)
        _run_patrol_window("run4_pre_scrub_patrol")
        gt_s1 = _game_time(h)
        h.rreturn("scrub start")
        h.wait(2)
        h.rreturn(f"scrub rewind_ticks {gt_s1 - gt_s0}")
        h.wait(2)
        h.rreturn("scrub commit")
        h.wait(2)
        h.rreturn("scrub off")
        h.wait(2)
        patrol_scrub = _run_patrol_window("scrub_patrol")
        _assert_records_match("Scrub commit PatrolSpawner schedule", patrol_scrub, patrol_run1)
    finally:
        try:
            h.rreturn("scrub cancel")
        except Exception:
            pass
        h.rreturn("scrub off")
        h.command("tick unfreeze")
        h.command("effect clear @s")
        h.command("time set day")
        h.command("gamerule spawn_mobs true")
        h.command("kill @e[type=!player,distance=..100]")
        h.command("fill 2560 65 2560 2640 76 2640 minecraft:air")
        h.command("gamemode survival")
        h.wait(2)
