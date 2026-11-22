"""Test 83 & 84: Interactive time scrubbing, wheel rewind, and mob motion freeze."""

import json
import re
from . import setup_clean_env


def _parse_res(res):
    if isinstance(res, dict):
        return res
    if isinstance(res, str):
        try:
            return json.loads(res)
        except Exception:
            start = res.find("{")
            if start != -1:
                return json.loads(res[start:])
    return {}


def _parse_pos(chat_str):
    nums = re.findall(r"[-+]?\d*\.?\d+(?:[eE][-+]?\d+)?", str(chat_str))
    if len(nums) >= 3:
        return [float(nums[0]), float(nums[1]), float(nums[2])]
    return None


def _parse_health(chat_str):
    s = str(chat_str)
    if ":" in s:
        s = s.split(":")[-1]
    nums = re.findall(r"[-+]?\d*\.?\d+", s)
    return float(nums[0]) if nums else 20.0


def test_83_interactive_time_scrubbing_and_rewind(h):
    setup_clean_env(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # 1. Verify that scrub is disabled initially after clean env
    st_init = _parse_res(h.rreturn("scrub status"))
    assert not st_init.get("enabled", False), f"Scrub should be disabled initially: {st_init}"
    assert not st_init.get("active", False), f"Scrub should be inactive initially: {st_init}"

    # Verify that trying to start scrub when disabled fails
    res_disabled = str(h.rreturn("scrub start"))
    assert "disabled" in res_disabled.lower() or "отключен" in res_disabled.lower(), (
        f"Expected disabled scrub error, got: {res_disabled}"
    )

    # 2. Enable scrub via settings / command
    res_on = str(h.rreturn("scrub on"))
    assert "on" in res_on.lower() or "вкл" in res_on.lower() or "enabled" in res_on.lower(), f"Unexpected scrub on response: {res_on}"
    st = _parse_res(h.rreturn("scrub status"))
    assert st.get("enabled", False), f"Scrub should be enabled now: {st}"
    assert not st.get("active", False), f"Scrub should be inactive initially: {st}"

    # Set a base checkpoint
    h.rreturn("set")
    h.wait(6)

    # Spawn a zombie, particles, and change diamond block
    h.command("summon minecraft:zombie 3.5 65.0 3.5 {Tags:['scrub_zombie'],CustomName:'\"ScrubZ\"'}")
    h.command("setblock 0 65 2 minecraft:diamond_block")
    h.command("particle minecraft:crit 3.5 66.0 3.5 0.2 0.2 0.2 0.1 15")
    h.wait(6)
    assert h.has_entity("@e[tag=scrub_zombie]"), "Zombie did not spawn"
    assert "diamond_block" in h.block(0, 65, 2).get("block", ""), "Diamond block not set"

    # Move zombie to distant location and break diamond block
    h.command("tp @e[tag=scrub_zombie] 15.5 65.0 15.5")
    h.command("setblock 0 65 2 minecraft:air")
    h.wait(14)
    assert "air" in h.block(0, 65, 2).get("block", ""), "Diamond block was not removed"
    far_pos = h.data_get_entity("@e[tag=scrub_zombie,limit=1]", "Pos")
    p_far = _parse_pos(far_pos)
    assert p_far and (p_far[0] > 10.0 or p_far[2] > 10.0), f"Zombie did not move to far position: {far_pos}"

    # Start interactive scrub mode
    res = h.rreturn("scrub start")
    res_str = str(res)
    assert "started" in res_str.lower(), f"Unexpected scrub start response: {res}"

    st = _parse_res(h.rreturn("scrub status"))
    assert st.get("active", False), f"Scrub should be active after start: {st}"
    anchor_t = st.get("anchor_tick", 0)

    # Step wheel backward past the break: diamond block should reappear in real time!
    h.rreturn("scrub step -14.0")
    st = _parse_res(h.rreturn("scrub status"))
    assert st.get("current_time", anchor_t) <= anchor_t, f"Cursor did not move back: {st}"
    assert "diamond_block" in h.block(0, 65, 2).get("block", ""), "Diamond block did not reappear in real-time scrubbing!"
    assert "particles" in st, f"Particles field expected in scrub status: {st}"
    assert st.get("particles", 0) > 0, f"Expected particles to be active during rewound scrub frame: {st}"
    assert "sounds" in st, f"Sounds field expected in scrub status: {st}"

    # Step forward: diamond block should disappear again in real time!
    h.rreturn("scrub step 12.0")
    assert "air" in h.block(0, 65, 2).get("block", ""), "Diamond block did not disappear on forward scrubbing!"

    # Step backward again and commit
    h.rreturn("scrub step -15.0")
    res_commit = str(h.rreturn("scrub commit"))
    assert "committed" in res_commit.lower(), f"Unexpected scrub commit response: {res_commit}"

    st = _parse_res(h.rreturn("scrub status"))
    assert not st.get("active", True), f"Scrub should be inactive after commit: {st}"

    # Verify that the zombie remained at the rewound position and did NOT teleport back to 15.5!
    h.wait(10)
    rewound_pos = h.data_get_entity("@e[tag=scrub_zombie,limit=1]", "Pos")
    assert "15." not in rewound_pos, f"Zombie snapped back to future position after commit: {rewound_pos}"

    # Test cancel workflow
    h.rreturn("scrub start")
    h.rreturn("scrub step -10.0")
    h.rreturn("scrub cancel")
    st = _parse_res(h.rreturn("scrub status"))
    assert not st.get("active", True), f"Scrub should be inactive after cancel: {st}"

    # Clean up and verify disabling scrub
    h.command("kill @e[tag=scrub_zombie]")
    h.command("setblock 0 65 2 minecraft:air")
    h.wait(4)
    h.rreturn("scrub off")
    st_off = _parse_res(h.rreturn("scrub status"))
    assert not st_off.get("enabled", False), f"Scrub should be disabled after off: {st_off}"


def test_84_scrub_mob_motion_freeze_and_no_extrapolation(h):
    setup_clean_env(h)
    h.rreturn("scrub on")
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Clean any leftover entities
    h.command("kill @e[tag=scrub_villager]")
    h.wait(2)

    # Summon a moving villager and let history record 14 ticks of active forward motion
    h.command("summon minecraft:villager 0.5 65.0 5.0 {Tags:['scrub_villager'],NoAI:0b,Motion:[0.0,0.0,0.25]}")
    h.wait(14)
    assert h.has_entity("@e[tag=scrub_villager]"), "Villager did not spawn"

    # Enter interactive scrub
    res = h.rreturn("scrub start")
    assert "started" in str(res).lower(), f"Unexpected scrub start response: {res}"

    # Capture position right at the start of scrub
    raw_start = h.data_get_entity("@e[tag=scrub_villager,limit=1]", "Pos")
    pos_start = _parse_pos(raw_start)
    assert pos_start is not None, f"Could not parse entity Pos at scrub start: {raw_start}"
    z_start = pos_start[2]

    # During scrub, the server clock MUST be frozen: wait 6 ticks and verify mob did NOT advance at all!
    h.wait(6)
    raw_frozen = h.data_get_entity("@e[tag=scrub_villager,limit=1]", "Pos")
    pos_frozen = _parse_pos(raw_frozen)
    assert pos_frozen is not None, f"Could not parse entity Pos during scrub: {raw_frozen}"
    z_frozen = pos_frozen[2]

    # In unpatched version, server clock ticked and Z increased by ~0.5 - 1.0 block forward.
    # With the fix, server is frozen and velocity is cleared, so z_frozen == z_start!
    assert abs(z_frozen - z_start) < 0.001, (
        f"Mob advanced forward during scrub freeze! At start: {z_start}, After wait: {z_frozen}"
    )

    # Step backward and commit rewound timeline
    h.rreturn("scrub step -8.0")
    res_commit = str(h.rreturn("scrub commit"))
    assert "committed" in res_commit.lower(), f"Unexpected scrub commit response: {res_commit}"
    h.wait(1)

    raw_rewound = h.data_get_entity("@e[tag=scrub_villager,limit=1]", "Pos")
    pos_rewound = _parse_pos(raw_rewound)
    assert pos_rewound is not None, f"Could not parse rewound Pos: {raw_rewound}"
    z_rewound = pos_rewound[2]
    assert z_rewound < z_start - 0.05, (
        f"Mob did not rewind backwards after commit! Start Z: {z_start}, Rewound Z: {z_rewound}"
    )

    # Cleanup
    h.command("kill @e[tag=scrub_villager]")
    h.wait(4)


def test_85_scrub_fov_bobbing_and_hurt_overlay(h):
    setup_clean_env(h)
    h.rreturn("scrub on")
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # 1. Test mob hurt overlay rewind
    h.command("kill @e[tag=scrub_cow]")
    h.command("summon minecraft:cow 0.5 65.0 4.0 {Tags:['scrub_cow']}")
    h.wait(20)
    assert h.has_entity("@e[tag=scrub_cow]"), "Cow did not spawn"

    # Deal damage to set hurt_time > 0
    h.command("damage @e[tag=scrub_cow,limit=1] 2")
    h.wait(2)
    t_dmg_str = str(h.command("time query gametime"))
    dmg_nums = re.findall(r"\d+", t_dmg_str)
    dmg_tick = int(dmg_nums[0]) if dmg_nums else 0

    ht_during_damage = h.data_get_entity("@e[tag=scrub_cow,limit=1]", "HurtTime")
    nums = re.findall(r"\d+", ht_during_damage)
    ht_val = int(nums[0]) if nums else 0
    assert ht_val > 0, f"Expected hurt time > 0 right after damage: {ht_during_damage}"

    # Wait until hurt_time expires to 0
    h.wait(10)
    ht_recovered = h.data_get_entity("@e[tag=scrub_cow,limit=1]", "HurtTime")
    nums = re.findall(r"\d+", ht_recovered)
    ht_rec_val = int(nums[0]) if nums else -1
    assert ht_rec_val == 0, f"Expected hurt time to recover to 0: {ht_recovered}"

    # Start scrub and step backwards to the exact moment of impact
    res = h.rreturn("scrub start")
    assert "started" in str(res).lower(), f"Unexpected scrub start: {res}"

    status = _parse_res(h.rreturn("scrub status"))
    anchor_tick = status.get("anchor_tick", 0)
    if anchor_tick > dmg_tick and dmg_tick > 0:
        step_val = -float(anchor_tick - dmg_tick) / 1.5
    else:
        step_val = -10.0

    h.rreturn(f"scrub step {step_val:.2f}")
    res_commit = str(h.rreturn("scrub commit"))
    assert "committed" in res_commit.lower(), f"Unexpected commit: {res_commit}"
    h.wait(2)

    # The cow's hurt time must be restored to > 0 on the server!
    ht_rewound = h.data_get_entity("@e[tag=scrub_cow,limit=1]", "HurtTime")
    nums = re.findall(r"\d+", ht_rewound)
    ht_rewound_val = int(nums[0]) if nums else 0
    assert ht_rewound_val > 0, f"Cow hurt_time was not restored on rewind: {ht_rewound}"

    # Cleanup cow
    h.command("kill @e[tag=scrub_cow]")
    h.wait(2)

    # 2. Test FOV and walk_dist telemetry in scrub status
    h.command("effect give @s minecraft:speed 10 3 true")
    h.wait(8)
    st_fast = _parse_res(h.rreturn("scrub status"))
    fov_fast = st_fast.get("fov_modifier", 1.0)
    assert fov_fast > 1.05, f"Expected higher FOV modifier with speed: {fov_fast}"

    h.command("effect clear @s")
    h.wait(12)
    st_normal = _parse_res(h.rreturn("scrub status"))
    fov_normal = st_normal.get("fov_modifier", 1.0)
    assert fov_normal < fov_fast, f"Expected FOV modifier to return to normal: {fov_normal}"

    # Start scrub and step back into the high speed / high FOV window
    h.rreturn("scrub start")
    h.rreturn("scrub step -8.0")
    st_rewound_fov = _parse_res(h.rreturn("scrub status"))
    fov_rewound = st_rewound_fov.get("fov_modifier", 1.0)
    assert fov_rewound > fov_normal, f"Expected FOV to expand on rewind to speed burst: {fov_rewound} vs normal {fov_normal}"

    h.rreturn("scrub cancel")
    h.wait(2)


def test_86_scrub_player_inventory_health_and_effects_no_dupe(h):
    setup_clean_env(h)
    h.rreturn("scrub on")
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # 1. Clear inventory and give baseline items
    h.command("clear @s")
    h.command("give @s minecraft:diamond 5")
    h.command("effect clear @s")
    h.command("effect give @s minecraft:instant_health 1 5 true")
    h.wait(15)

    # Check baseline diamonds count
    raw_inv = h.data_get_entity("@s", "Inventory")
    assert "diamond" in str(raw_inv), f"Expected diamonds in inventory: {raw_inv}"
    assert "Count: 5b" in str(raw_inv) or "count: 5" in str(raw_inv).lower(), f"Expected 5 diamonds: {raw_inv}"

    # 2. Give 10 more diamonds (total 15), damage player, and give poison
    h.command("give @s minecraft:diamond 10")
    h.command("damage @s 4")
    h.command("effect give @s minecraft:poison 6 0")
    h.wait(10)

    raw_inv_more = h.data_get_entity("@s", "Inventory")
    assert "Count: 15b" in str(raw_inv_more) or "count: 15" in str(raw_inv_more).lower(), f"Expected 15 diamonds: {raw_inv_more}"
    hp_damaged = h.data_get_entity("@s", "Health")
    hp_val = _parse_health(hp_damaged)
    assert hp_val < 19.0, f"Expected damaged HP: {hp_val} (raw: {hp_damaged})"

    # 3. Start scrub and step backward ~16 steps before the damage and extra diamonds
    res = h.rreturn("scrub start")
    assert "started" in str(res).lower(), f"Unexpected scrub start: {res}"
    h.rreturn("scrub step -16.0")
    res_commit = str(h.rreturn("scrub commit"))
    assert "committed" in res_commit.lower(), f"Unexpected scrub commit: {res_commit}"
    h.wait(4)

    # 4. Verify rollback:
    # Inventory must have EXACTLY 5 diamonds (no dupe, no 15 diamonds)!
    raw_inv_rewound = h.data_get_entity("@s", "Inventory")
    assert "Count: 5b" in str(raw_inv_rewound) or "count: 5" in str(raw_inv_rewound).lower(), (
        f"Diamonds were not rolled back to 5! Dupe occurred: {raw_inv_rewound}"
    )
    assert "Count: 15b" not in str(raw_inv_rewound), f"Future diamonds remained: {raw_inv_rewound}"

    # Health must be restored to 20!
    hp_rewound = h.data_get_entity("@s", "Health")
    hp_rewound_val = _parse_health(hp_rewound)
    assert hp_rewound_val >= 19.9, f"HP was not restored: {hp_rewound_val} (raw: {hp_rewound})"

    # Poison effect must be gone!
    raw_effects = h.data_get_entity("@s", "active_effects")
    assert "poison" not in str(raw_effects).lower(), f"Poison effect was not removed: {raw_effects}"


def test_87_scrub_mob_resurrection_and_tnt_explosion_revert(h):
    setup_clean_env(h)
    h.rreturn("scrub on")
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # 1. Clean previous entities
    h.command("kill @e[tag=scrub_target_zombie]")
    h.command("setblock 5 65 5 minecraft:emerald_block")
    h.wait(4)
    assert "emerald_block" in h.block(5, 65, 5).get("block", ""), "Emerald block not placed"

    # 2. Summon zombie at (3.5, 65, 3.5) and record history
    h.command("summon minecraft:zombie 3.5 65.0 3.5 {Tags:['scrub_target_zombie'],CustomName:'\"UndeadScrub\"'}")
    h.wait(20)
    assert h.has_entity("@e[tag=scrub_target_zombie]"), "Zombie did not spawn"

    # 3. Kill the zombie and destroy the emerald block
    h.command("kill @e[tag=scrub_target_zombie]")
    h.command("setblock 5 65 5 minecraft:air")
    h.wait(10)
    assert not h.has_entity("@e[tag=scrub_target_zombie]"), "Zombie was not killed"
    assert "air" in h.block(5, 65, 5).get("block", ""), "Emerald block was not destroyed"

    # 4. Start scrub and step backward ~12 steps (~18 ticks) to when zombie was alive and emerald was present
    res = h.rreturn("scrub start")
    assert "started" in str(res).lower(), f"Unexpected scrub start: {res}"
    h.rreturn("scrub step -12.0")
    res_commit = str(h.rreturn("scrub commit"))
    assert "committed" in res_commit.lower(), f"Unexpected scrub commit: {res_commit}"
    h.wait(4)

    # 5. Zombie MUST be resurrected!
    assert h.has_entity("@e[tag=scrub_target_zombie]"), "Dead zombie was NOT resurrected on scrub commit!"
    # Emerald block must be restored!
    assert "emerald_block" in h.block(5, 65, 5).get("block", ""), "Emerald block was NOT restored on scrub commit!"

    # Cleanup
    h.command("kill @e[tag=scrub_target_zombie]")
    h.command("setblock 5 65 5 minecraft:air")
    h.wait(2)




