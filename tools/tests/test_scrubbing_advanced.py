"""Advanced scrubbing E2E tests covering mob motion, inventory, containers, weather, and CLI."""

import json
import re
from . import setup_clean_env as _base_clean_env


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


def _setup_scrub(h):
    _base_clean_env(h)
    h.rreturn("scrub on")


def test_88_scrub_continuous_mob_motion_no_stutter(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    h.command("kill @e[tag=test88_zombie]")
    h.wait(2)

    # 1. Spawn zombie at initial position Z=2.0
    h.command("summon minecraft:zombie 0.5 65.0 2.0 {Tags:['test88_zombie'],NoAI:1b}")
    h.wait(15)
    assert h.has_entity("@e[tag=test88_zombie]"), "Test 88 zombie did not spawn"

    # 2. Advance zombie forward to Z=8.0
    h.command("tp @e[tag=test88_zombie] 0.5 65.0 8.0")
    h.wait(10)

    raw_anchor = h.data_get_entity("@e[tag=test88_zombie,limit=1]", "Pos")
    pos_anchor = _parse_pos(raw_anchor)
    assert pos_anchor is not None, f"Could not parse Pos at anchor: {raw_anchor}"
    z_anchor = pos_anchor[2]
    assert z_anchor >= 7.5, f"Zombie did not move to target pos: {z_anchor}"

    # 3. Enter scrub mode and step backward ~12 steps past the teleport
    res = h.rreturn("scrub start")
    assert "started" in str(res).lower(), f"Unexpected scrub start response: {res}"
    h.rreturn("scrub step -12.0")
    h.rreturn("scrub commit")
    h.wait(4)

    raw_rewound = h.data_get_entity("@e[tag=test88_zombie,limit=1]", "Pos")
    pos_rewound = _parse_pos(raw_rewound)
    assert pos_rewound is not None, f"Could not parse rewound Pos: {raw_rewound}"
    z_rewound = pos_rewound[2]

    # Z position must have rewound back to initial Z ~ 2.0
    assert z_rewound < z_anchor - 4.0, (
        f"Zombie position did not rewind back: anchor={z_anchor}, rewound={z_rewound}"
    )

    h.command("kill @e[tag=test88_zombie]")
    h.wait(2)


def test_89_scrub_armor_and_equipment_rollback(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Baseline: empty armor
    h.command("clear @s")
    h.wait(10)

    # Give diamond helmet and shield
    h.command("give @s minecraft:diamond_helmet")
    h.command("give @s minecraft:shield")
    h.wait(15)

    raw_armor = str(h.data_get_entity("@s", "Inventory"))
    assert "diamond_helmet" in raw_armor, f"Helmet not given: {raw_armor}"
    assert "shield" in raw_armor, f"Shield not given: {raw_armor}"

    # Scrub backward ~14 steps past the equipping
    h.rreturn("scrub start")
    h.rreturn("scrub step -14.0")
    h.rreturn("scrub commit")
    h.wait(4)

    # Inventory must be empty again
    raw_armor_after = str(h.data_get_entity("@s", "Inventory"))
    assert "diamond_helmet" not in raw_armor_after, f"Helmet remained in inventory: {raw_armor_after}"
    assert "shield" not in raw_armor_after, f"Shield remained in inventory: {raw_armor_after}"


def test_90_scrub_experience_level_and_progress_rollback(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Baseline: 0 XP
    h.command("xp set @s 0 levels")
    h.command("xp set @s 0 points")
    h.wait(10)

    # Add 25 levels
    h.command("xp add @s 25 levels")
    h.wait(12)
    raw_xp = str(h.data_get_entity("@s", "XpLevel"))
    assert "25" in raw_xp, f"Expected 25 XP levels: {raw_xp}"

    # Scrub back 14 steps
    h.rreturn("scrub start")
    h.rreturn("scrub step -14.0")
    h.rreturn("scrub commit")
    h.wait(4)

    raw_xp_after = str(h.data_get_entity("@s", "XpLevel"))
    nums = re.findall(r"\d+", raw_xp_after.split(":")[-1])
    lvl = int(nums[0]) if nums else -1
    assert lvl == 0, f"XP level was not reverted to 0: {raw_xp_after}"


def test_91_scrub_dropped_item_pickup_and_reemergence(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    h.command("kill @e[tag=test91_item]")
    h.wait(2)

    # Summon dropped item at (2.5, 65.0, 2.5)
    h.command("summon minecraft:item 2.5 65.0 2.5 {Tags:['test91_item'],Item:{id:\"minecraft:netherite_ingot\",count:3}}")
    h.wait(15)
    assert h.has_entity("@e[tag=test91_item]"), "Dropped item did not spawn"

    # Remove the item
    h.command("kill @e[tag=test91_item]")
    h.wait(10)
    assert not h.has_entity("@e[tag=test91_item]"), "Item was not killed"

    # Scrub back and commit
    h.rreturn("scrub start")
    h.rreturn("scrub step -14.0")
    h.rreturn("scrub commit")
    h.wait(4)

    # Item must re-emerge
    assert h.has_entity("@e[tag=test91_item]"), "Dropped item did NOT re-emerge on scrub rewind!"
    h.command("kill @e[tag=test91_item]")
    h.wait(2)


def test_92_scrub_fire_ticks_and_extinguish_rollback(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    h.command("kill @e[tag=test92_cow]")
    h.command("weather clear")
    h.command("setblock 0 67 2 minecraft:stone")
    h.wait(2)

    # Summon cow with NoAI under ceiling
    h.command("summon minecraft:cow 0.5 65.0 2.0 {Tags:['test92_cow'],NoAI:1b}")
    h.wait(10)
    assert h.has_entity("@e[tag=test92_cow]"), "Test 92 cow did not spawn"

    h.command("data modify entity @e[tag=test92_cow,limit=1] Fire set value 200s")
    h.wait(10)
    raw_fire = str(h.data_get_entity("@e[tag=test92_cow,limit=1]", "Fire"))
    nums = re.findall(r"[-+]?\d+", raw_fire.split(":")[-1])
    fire_val = int(nums[0]) if nums else 0
    assert fire_val > 50, f"Expected fire ticks > 50: {raw_fire}"

    # Extinguish cow fire
    h.command("data modify entity @e[tag=test92_cow,limit=1] Fire set value 0s")
    h.wait(10)
    raw_fire_ext = str(h.data_get_entity("@e[tag=test92_cow,limit=1]", "Fire"))
    nums_ext = re.findall(r"[-+]?\d+", raw_fire_ext.split(":")[-1])
    fire_ext_val = int(nums_ext[0]) if nums_ext else 0
    assert fire_ext_val <= 0, f"Expected fire extinguished: {raw_fire_ext}"

    # Scrub back into burning period
    h.rreturn("scrub start")
    h.rreturn("scrub step -12.0")
    h.rreturn("scrub commit")
    h.wait(4)

    raw_fire_restored = str(h.data_get_entity("@e[tag=test92_cow,limit=1]", "Fire"))
    nums_res = re.findall(r"[-+]?\d+", raw_fire_restored.split(":")[-1])
    fire_res_val = int(nums_res[0]) if nums_res else 0
    assert fire_res_val > 20, f"Fire was not restored on rewind: {raw_fire_restored}"

    # Cleanup
    h.command("kill @e[tag=test92_cow]")
    h.command("setblock 0 67 2 minecraft:air")
    h.command("weather clear")
    h.wait(2)


def test_93_scrub_rapid_toggle_and_cancel_stability(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Perform 5 rapid start-cancel cycles
    for _ in range(5):
        res_s = h.rreturn("scrub start")
        assert "started" in str(res_s).lower(), f"Failed start in cycle: {res_s}"
        h.rreturn("scrub step -2.0")
        res_c = h.rreturn("scrub cancel")
        assert "cancelled" in str(res_c).lower(), f"Failed cancel in cycle: {res_c}"

    st = _parse_res(h.rreturn("scrub status"))
    assert not st.get("active", False), f"Scrub should not be active: {st}"

    # Ensure server tick clock is running normally and mob motion advances
    h.command("kill @e[tag=test93_pig]")
    h.command("summon minecraft:pig 0.5 65.0 2.0 {Tags:['test93_pig'],Motion:[0.0,0.0,0.3],NoAI:0b}")
    h.wait(8)
    raw_pos = h.data_get_entity("@e[tag=test93_pig,limit=1]", "Pos")
    pos = _parse_pos(raw_pos)
    assert pos is not None and pos[2] > 2.05, f"Pig did not move, clock frozen: {raw_pos}"
    h.command("kill @e[tag=test93_pig]")
    h.wait(2)


def test_94_scrub_container_furnace_cook_time_rollback(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Set furnace with Items and lit=true
    h.command("setblock 2 65 0 minecraft:furnace[lit=true]{Items:[{Slot:0b,id:'minecraft:raw_iron',count:10},{Slot:1b,id:'minecraft:coal',count:4}],cooking_time_spent:80s,lit_time_remaining:120s}")
    h.wait(12)
    items_val = str(h.data_get_block(2, 65, 0, "Items"))
    assert "raw_iron" in items_val, f"Furnace raw_iron not set: {items_val}"

    # Consume raw iron and clear furnace
    h.command("setblock 2 65 0 minecraft:furnace[lit=false]{Items:[{Slot:2b,id:'minecraft:iron_ingot',count:10}],cooking_time_spent:0s,lit_time_remaining:0s}")
    h.wait(10)
    items_mut = str(h.data_get_block(2, 65, 0, "Items"))
    assert "raw_iron" not in items_mut, f"Furnace items not cleared: {items_mut}"

    # Scrub back and commit
    h.rreturn("scrub start")
    h.rreturn("scrub step -12.0")
    h.rreturn("scrub commit")
    h.wait(4)

    items_rewound = str(h.data_get_block(2, 65, 0, "Items"))
    assert "raw_iron" in items_rewound, f"Furnace raw_iron was not restored on scrub rewind: {items_rewound}"

    h.command("setblock 2 65 0 minecraft:air")
    h.wait(2)


def test_95_scrub_redstone_lamp_power_toggle_rollback(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Place redstone lamp powered by redstone block
    h.command("setblock 2 65 1 minecraft:redstone_lamp")
    h.command("setblock 2 65 0 minecraft:redstone_block")
    h.wait(10)
    lamp_lit = h.block(2, 65, 1).get("block", "")
    assert "lit=true" in lamp_lit or "redstone_lamp" in lamp_lit, f"Lamp not powered: {lamp_lit}"

    # Remove power source
    h.command("setblock 2 65 0 minecraft:air")
    h.wait(10)
    lamp_unlit = h.block(2, 65, 1).get("block", "")
    assert "lit=false" in lamp_unlit or "air" in h.block(2, 65, 0).get("block", ""), f"Lamp power not removed: {lamp_unlit}"

    # Scrub back and commit
    h.rreturn("scrub start")
    h.rreturn("scrub step -12.0")
    h.rreturn("scrub commit")
    h.wait(4)

    assert "redstone_block" in h.block(2, 65, 0).get("block", ""), "Redstone block was not restored on scrub rewind!"
    h.command("setblock 2 65 1 minecraft:air")
    h.command("setblock 2 65 0 minecraft:air")
    h.wait(2)


def test_96_scrub_mass_block_fill_and_restore(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Place obsidian volume
    h.command("fill 1 65 1 3 67 3 minecraft:obsidian")
    h.wait(12)
    assert "obsidian" in h.block(2, 66, 2).get("block", ""), "Obsidian fill not placed"

    # Replace with gold blocks
    h.command("fill 1 65 1 3 67 3 minecraft:gold_block")
    h.wait(12)
    assert "gold_block" in h.block(2, 66, 2).get("block", ""), "Gold block fill not placed"

    # Scrub back and commit
    h.rreturn("scrub start")
    h.rreturn("scrub step -14.0")
    h.rreturn("scrub commit")
    h.wait(4)

    assert "obsidian" in h.block(2, 66, 2).get("block", ""), "Obsidian fill volume was NOT restored on rewind!"
    h.command("fill 1 65 1 3 67 3 minecraft:air")
    h.wait(2)


def test_97_scrub_weather_rain_and_thunder_rollback(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Baseline clear
    h.command("weather clear")
    h.wait(10)
    st_clear = _parse_res(h.rreturn("scrub status"))
    assert st_clear.get("rain_level", 0.0) < 0.1, f"Expected clear rain level: {st_clear}"

    # Change to rain/thunder
    h.command("weather thunder")
    h.wait(15)

    # Scrub back to clear
    h.rreturn("scrub start")
    h.rreturn("scrub step -15.0")
    h.rreturn("scrub commit")
    h.wait(4)

    st_after = _parse_res(h.rreturn("scrub status"))
    assert st_after.get("rain_level", 0.0) < 0.1, f"Weather was not restored to clear: {st_after}"


def test_98_scrub_flying_arrow_trajectory_rollback(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    h.command("kill @e[tag=test98_arrow]")
    h.wait(2)

    # Summon arrow shooting upwards
    h.command("summon minecraft:arrow 0.5 70.0 0.5 {Tags:['test98_arrow'],Motion:[0.0,0.6,0.0]}")
    h.wait(6)
    raw_mid = h.data_get_entity("@e[tag=test98_arrow,limit=1]", "Pos")
    pos_mid = _parse_pos(raw_mid)
    assert pos_mid is not None, f"Could not parse arrow mid Pos: {raw_mid}"

    h.wait(12)
    raw_late = h.data_get_entity("@e[tag=test98_arrow,limit=1]", "Pos")
    pos_late = _parse_pos(raw_late)

    # Scrub back to mid-flight
    h.rreturn("scrub start")
    h.rreturn("scrub step -8.0")
    h.rreturn("scrub commit")
    h.wait(4)

    raw_rewound = h.data_get_entity("@e[tag=test98_arrow,limit=1]", "Pos")
    pos_rewound = _parse_pos(raw_rewound)
    assert pos_rewound is not None, f"Could not parse rewound arrow: {raw_rewound}"

    # Position must be close to pos_mid rather than pos_late
    assert abs(pos_rewound[1] - pos_mid[1]) < 1.0, (
        f"Arrow trajectory did not rewind: rewound Y={pos_rewound[1]}, mid Y={pos_mid[1]}, late Y={pos_late[1] if pos_late else 'dead'}"
    )

    h.command("kill @e[tag=test98_arrow]")
    h.wait(2)


def test_99_scrub_cursor_bounds_clamping(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    h.rreturn("scrub start")
    st = _parse_res(h.rreturn("scrub status"))
    assert st.get("active", False), f"Expected active scrub: {st}"
    min_t = st.get("min_tick", 0)
    anchor_t = st.get("anchor_tick", 0)

    # Over-step backwards: must clamp to min_tick
    h.rreturn("scrub step -9999.0")
    st_back = _parse_res(h.rreturn("scrub status"))
    curr_back = st_back.get("current_time", -1)
    assert abs(curr_back - min_t) < 0.1, f"Cursor did not clamp to min_tick: {curr_back} vs {min_t}"

    # Over-step forwards: must clamp to anchor_tick
    h.rreturn("scrub step 9999.0")
    st_fwd = _parse_res(h.rreturn("scrub status"))
    curr_fwd = st_fwd.get("current_time", -1)
    assert abs(curr_fwd - anchor_t) < 0.1, f"Cursor did not clamp to anchor_tick: {curr_fwd} vs {anchor_t}"

    h.rreturn("scrub cancel")
    h.wait(2)


def test_100_scrub_rewind_subcommand_full_cycle(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Place diamond block
    h.command("setblock 1 65 1 minecraft:diamond_block")
    h.wait(16)
    assert "diamond_block" in h.block(1, 65, 1).get("block", ""), "Diamond block not placed"

    # Break diamond block
    h.command("setblock 1 65 1 minecraft:air")
    h.wait(8)
    assert "air" in h.block(1, 65, 1).get("block", ""), "Diamond block was not broken"

    # Use single-line /rreturn scrub rewind_ticks 14 command
    res = h.rreturn("scrub rewind_ticks 14")
    assert "scrubbed back" in str(res).lower(), f"Unexpected rewind response: {res}"
    h.wait(4)

    # Block must be restored
    assert "diamond_block" in h.block(1, 65, 1).get("block", ""), "Diamond block was NOT restored by /rreturn scrub rewind_ticks!"

    h.command("setblock 1 65 1 minecraft:air")
    h.wait(2)


def test_101_scrub_dispenser_prng_selection_determinism(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Place dispenser with emerald items
    h.command(
        "setblock 2 65 0 minecraft:dispenser[facing=north]{Items:["
        "{Slot:0b,id:'minecraft:emerald',count:5}"
        "]}"
    )
    h.wait(15)

    # Trigger dispenser with redstone
    h.command("setblock 2 66 0 minecraft:redstone_block")
    h.wait(6)

    dropped_1 = str(h.data_get_entity("@e[type=item,limit=1,distance=..6]", "Item.id"))
    assert "emerald" in dropped_1, f"No emerald dispensed on first run: {dropped_1}"
    h.command("kill @e[type=item]")
    h.command("setblock 2 66 0 minecraft:air")

    # Scrub back to before the redstone block was placed
    h.rreturn("scrub start")
    h.rreturn("scrub step -12.0")
    h.rreturn("scrub commit")
    h.wait(4)

    # Verify dispenser item count was restored to 5
    raw_inv = str(h.data_get_block(2, 65, 0, "Items"))
    assert "count: 5" in raw_inv.lower() or "count: 5b" in raw_inv.lower(), (
        f"Dispenser items not restored: {raw_inv}"
    )

    # Trigger dispenser again
    h.command("setblock 2 66 0 minecraft:redstone_block")
    h.wait(6)

    dropped_2 = str(h.data_get_entity("@e[type=item,limit=1,distance=..6]", "Item.id"))
    h.command("kill @e[type=item]")
    h.command("setblock 2 65 0 minecraft:air")
    h.command("setblock 2 66 0 minecraft:air")

    # The exact same item must be dispensed deterministically!
    assert dropped_1 == dropped_2, (
        f"Dispenser output deviated across scrub: {dropped_1} vs {dropped_2}"
    )


def test_102_scrub_chest_loot_table_seed_determinism(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Place chest with loot table and seed
    h.command("setblock 2 65 2 minecraft:chest{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:133742L}")
    h.wait(12)
    loot_tag = str(h.data_get_block(2, 65, 2, "LootTable"))
    assert "simple_dungeon" in loot_tag, f"LootTable not set: {loot_tag}"

    # Replace chest with gold block
    h.command("setblock 2 65 2 minecraft:gold_block")
    h.wait(10)
    assert "gold_block" in h.block(2, 65, 2).get("block", ""), "Gold block not placed"

    # Scrub back and commit
    h.rreturn("scrub start")
    h.rreturn("scrub step -12.0")
    h.rreturn("scrub commit")
    h.wait(4)

    # Chest and its exact loot table & seed must be restored deterministically
    loot_restored = str(h.data_get_block(2, 65, 2, "LootTable"))
    seed_restored = str(h.data_get_block(2, 65, 2, "LootTableSeed"))
    assert "simple_dungeon" in loot_restored, f"LootTable was not restored on rewind: {loot_restored}"
    assert "133742" in seed_restored, f"LootTableSeed was not restored on rewind: {seed_restored}"

    h.command("setblock 2 65 2 minecraft:air")
    h.wait(2)


def test_103_scrub_tnt_fuse_and_deterministic_deflection(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Summon primed TNT
    h.command("kill @e[tag=test103_tnt]")
    h.command("summon minecraft:tnt 0.5 66.0 0.5 {Tags:['test103_tnt'],Fuse:100s,Motion:[0.1,0.2,0.1]}")
    h.wait(8)
    assert h.has_entity("@e[tag=test103_tnt]"), "Primed TNT did not spawn"

    # Scrub back 12 steps before TNT was summoned
    h.rreturn("scrub start")
    h.rreturn("scrub step -12.0")
    h.rreturn("scrub commit")
    h.wait(4)

    # TNT must be completely gone in the pre-ignition state
    assert not h.has_entity("@e[tag=test103_tnt]"), "Primed TNT remained after scrub rewind!"


def test_104_scrub_villager_ai_schedule_determinism(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Summon villager with workstation
    h.command("kill @e[tag=test104_vil]")
    h.command("setblock 2 65 2 minecraft:lectern")
    h.command("summon minecraft:villager 0.5 65.0 2.0 {Tags:['test104_vil'],VillagerData:{profession:'minecraft:librarian',level:1}}")
    h.wait(15)
    assert h.has_entity("@e[tag=test104_vil]"), "Villager did not spawn"

    # Scrub back to before lectern was placed
    h.rreturn("scrub start")
    h.rreturn("scrub step -14.0")
    h.rreturn("scrub commit")
    h.wait(4)

    # Lectern must be air, and villager should not exist
    assert "air" in h.block(2, 65, 2).get("block", ""), "Lectern was not rolled back to air"
    assert not h.has_entity("@e[tag=test104_vil]"), "Villager was not rolled back"


def test_107_scrub_seamless_continuity_mobs_particles_and_cracks(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    h.command("kill @e[tag=test107_persistent]")
    h.command("kill @e[tag=test107_future]")
    h.wait(2)

    # 1. Spawn persistent mob existing in the baseline frame
    h.command("summon minecraft:cow 1.5 65.0 1.5 {Tags:['test107_persistent'],NoAI:1b}")
    h.wait(10)
    assert h.has_entity("@e[tag=test107_persistent]"), "Persistent cow did not spawn"

    cow_pos_before = _parse_pos(h.data_get_entity("@e[tag=test107_persistent,limit=1]", "Pos"))
    assert cow_pos_before is not None

    # Spawn stone block and visual particles
    h.command("setblock 1 65 3 minecraft:stone")
    h.command("particle minecraft:flame 1.5 66.0 1.5 0.1 0.1 0.1 0.01 20")
    h.wait(10)

    # 2. Spawn future mob (spawned later in timeline)
    h.command("summon minecraft:pig 2.5 65.0 2.5 {Tags:['test107_future'],NoAI:1b}")
    h.wait(8)
    assert h.has_entity("@e[tag=test107_future]"), "Future pig did not spawn"

    # 3. Scrub backward to before the future pig spawned
    h.rreturn("scrub start")
    h.rreturn("scrub step -10.0")
    h.rreturn("scrub commit")
    h.wait(4)

    # 4. Verify seamless continuity:
    # - Persistent cow must continuously exist and not die or vanish
    assert h.has_entity("@e[tag=test107_persistent]"), "Persistent mob disappeared or died after scrub commit!"
    cow_pos_after = _parse_pos(h.data_get_entity("@e[tag=test107_persistent,limit=1]", "Pos"))
    assert cow_pos_after is not None
    assert abs(cow_pos_after[0] - 1.5) < 0.2 and abs(cow_pos_after[2] - 1.5) < 0.2, (
        f"Cow position drifted unexpectedly: {cow_pos_after}"
    )

    # - Future mob must be pruned cleanly
    assert not h.has_entity("@e[tag=test107_future]"), "Future mob was not pruned upon scrub commit!"

    # 5. Clean up
    h.command("kill @e[tag=test107_persistent]")
    h.command("setblock 1 65 3 minecraft:air")
    h.wait(2)


def test_108_scrub_villager_profession_and_mass_tnt_continuity(h):
    _setup_scrub(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    h.command("kill @e[tag=test108_vil]")
    h.wait(2)

    # 1. Spawn a baseline unemployed villager
    h.command("summon minecraft:villager 1.5 65.0 1.5 {VillagerData:{profession:\"minecraft:none\",level:1},Tags:['test108_vil'],NoAI:1b}")
    h.wait(30)
    assert h.has_entity("@e[tag=test108_vil]"), "Villager did not spawn"
    prof_before = h.data_get_entity("@e[tag=test108_vil,limit=1]", "VillagerData.profession")
    assert "none" in prof_before.lower(), f"Expected unemployed villager, got {prof_before}"

    # 2. Villager takes a profession (farmer, level 2)
    h.command("data merge entity @e[tag=test108_vil,limit=1] {VillagerData:{profession:\"minecraft:farmer\",level:2}}")
    h.wait(10)
    prof_mid = h.data_get_entity("@e[tag=test108_vil,limit=1]", "VillagerData.profession")
    assert "farmer" in prof_mid.lower(), f"Expected farmer villager, got {prof_mid}"

    # 3. Fill a mass region with TNT (10x10x10 blocks = 1000 blocks across sections)
    h.command("fill ~2 ~ ~2 ~11 ~9 ~11 minecraft:tnt")
    h.wait(10)
    tnt_block = h.block(2, 65, 2)
    assert "tnt" in tnt_block.get("block", "").lower(), f"Expected TNT block, got {tnt_block}"

    # 4. Scrub backward to before TNT was placed and before villager took farmer profession
    h.rreturn("scrub start")
    h.rreturn("scrub step -18.0")
    h.rreturn("scrub commit")
    h.wait(4)

    # 5. Verify villager profession is seamlessly reverted to none/unemployed
    assert h.has_entity("@e[tag=test108_vil]"), "Villager disappeared after scrub commit"
    prof_after = h.data_get_entity("@e[tag=test108_vil,limit=1]", "VillagerData.profession")
    assert "none" in prof_after.lower(), f"Expected villager profession reverted to none, got {prof_after}"
    lvl_after = h.data_get_entity("@e[tag=test108_vil,limit=1]", "VillagerData.level")
    assert "1" in lvl_after, f"Expected villager level 1, got {lvl_after}"

    # 6. Verify TNT fill is cleanly rolled back without holes
    block_restored = h.block(2, 65, 2)
    assert "tnt" not in block_restored.get("block", "").lower(), f"Expected TNT block removed, got {block_restored}"

    # 7. Clean up
    h.command("kill @e[tag=test108_vil]")
    h.command("fill ~2 ~ ~2 ~11 ~9 ~11 minecraft:air")
    h.wait(2)


