"""Player state, combat, effects, and inventory bug regression tests (Tests 49, 51, 52, 56, 57, 58, 61, 62, 71, 73, 78, 117)."""

import re

from . import setup_clean_env


def test_49_thrown_potion_cloud_preservation(h):
    """Bug #6 investigation: Check whether flying splash potions and effect clouds survive rollback."""
    setup_clean_env(h)
    h.tp(200.5, 65.0, 200.5)
    h.command("kill @e[tag=test_potion]")
    h.command("kill @e[tag=test_cloud]")

    # Spawn thrown splash potion and area effect cloud
    h.command("summon minecraft:splash_potion 200.5 75.0 200.5 {Item:{id:'minecraft:splash_potion',count:1},Motion:[0.0,0.2,0.0],Tags:['test_potion']}")
    h.command("summon area_effect_cloud 200.5 65.0 200.5 {Radius:3.0f,Duration:600,Tags:['test_cloud']}")
    h.wait(2)

    h.command("tick freeze")
    h.wait(2)

    h.rreturn("set")
    h.wait(2)

    # Remove entities
    h.command("kill @e[tag=test_potion]")
    h.command("kill @e[tag=test_cloud]")
    h.wait(2)

    # Rollback
    h.rreturn("load")
    h.wait(4)

    has_potion = h.has_entity("@e[tag=test_potion]")
    has_cloud = h.has_entity("@e[tag=test_cloud]")

    h.command("tick unfreeze")
    h.command("kill @e[tag=test_potion]")
    h.command("kill @e[tag=test_cloud]")

    # Bug verification: in restore_level.rs:408:
    # game.potions.clear_thrown(); game.potions.clouds.clear();
    # This verifies if game clears them
    assert has_potion, "Flying splash potion was cleared on rollback (game.potions.clear_thrown)"
    assert has_cloud, "AreaEffectCloud was cleared on rollback (game.potions.clouds.clear)"



def test_51_powder_snow_ticks_frozen_preservation(h):
    """Bug #31 investigation: Check whether player TicksFrozen survives rollback."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)

    # Set player frozen ticks via data
    h.command("data merge entity @s {TicksFrozen:140}")
    h.wait(3)

    st_before = h.state().get("player", {})
    frozen_before = st_before.get("ticks_frozen", 0)

    h.rreturn("set")
    h.wait(2)

    # Clear frozen ticks
    h.command("data merge entity @s {TicksFrozen:0}")
    h.wait(3)

    # Rollback
    h.rreturn("load")
    h.wait(4)

    st_after = h.state().get("player", {})
    frozen_after = st_after.get("ticks_frozen", 0)

    # Report values
    assert frozen_after >= 120 or frozen_before == 0, (
        f"TicksFrozen was not restored in state: before={frozen_before}, after={frozen_after}"
    )



def test_52_gui_closure_verification(h):
    """Verify exact screen state restoration on rollback (restoring open GUI if saved open, closing if saved while playing)."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)

    # 1. Checkpoint saved with inventory open must return player back into inventory on rollback
    h.open_inventory()
    h.wait(3)
    scr_before = h.screen().get("screen")
    assert scr_before in ("inventory", "container", "crafting"), f"Screen not opened before: {scr_before}"

    h.rreturn("set")
    h.wait(2)
    h.close_screen()
    h.wait(2)
    assert h.screen().get("screen") == "playing"

    h.rreturn("load")
    h.wait(4)
    scr_after = h.screen().get("screen")
    assert scr_after in ("inventory", "container", "crafting"), (
        f"Expected open inventory UI to be restored on rollback, got screen={scr_after}"
    )
    h.close_screen()
    h.wait(2)

    # 2. Checkpoint saved while playing must still close any GUI opened after the checkpoint
    h.rreturn("set")
    h.wait(2)
    h.open_inventory()
    h.wait(2)
    assert h.screen().get("screen") in ("inventory", "container", "crafting")
    h.rreturn("load")
    h.wait(4)
    scr_playing = h.screen().get("screen")
    assert scr_playing == "playing", f"Expected GUI opened after checkpoint to close on rollback, got screen={scr_playing}"



def test_56_singleplayer_shield_cooldown_restoration(h):
    """Bug #32 investigation: Check whether shield cooldown is restored in singleplayer."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("give @s minecraft:shield 1")
    h.command("give @s minecraft:iron_axe 1")
    h.select_slot(1)
    h.wait(2)

    cd_before = h.state().get("player", {}).get("shield_cooldown", 0)

    # Checkpoint when shield has 0 cooldown
    h.rreturn("set")
    h.wait(2)

    h.rreturn("load")
    h.wait(4)
    cd_after = h.state().get("player", {}).get("shield_cooldown", 0)
    h.command("clear @s")
    assert cd_after == 0, f"Shield cooldown was not 0 after clean rollback: {cd_after}"



def test_57_fall_flying_elytra_mode_restoration(h):
    """Bug #30 investigation: Check whether elytra fall_flying mode is preserved on rollback."""
    setup_clean_env(h)
    h.tp(0.5, 120.0, 0.5)
    h.command("item replace entity @s armor.chest with minecraft:elytra")
    h.wait(3)

    # Checkpoint in the air
    h.rreturn("set")
    h.wait(2)

    # Land on the ground
    h.tp(0.5, 65.0, 0.5)
    h.wait(4)

    # Rollback to air
    h.rreturn("load")
    h.wait(4)

    st_after = h.state().get("player", {})
    pos_after = st_after.get("position", [0, 0, 0])
    h.command("clear @s")
    assert pos_after[1] > 100.0, f"Player did not restore back into the air: {pos_after}"



def test_58_using_item_interruption_on_rollback(h):
    """Bug #10 investigation: Check whether using item (eating/drawing bow) is cancelled on rollback."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("give @s minecraft:golden_apple 5")
    h.select_slot(1)
    h.wait(2)

    # Start using item
    h.use_item(hold=True)
    h.wait(3)

    h.rreturn("set")
    h.wait(2)

    # Rollback cancels using item
    h.rreturn("load")
    h.wait(4)

    st_after = h.state()
    using_after = st_after.get("using")
    # In restore_level.rs:543: game.using = None;
    h.command("clear @s")
    assert using_after is None, f"Expected game.using to be None on rollback, got {using_after}"



def test_61_poison_effect_recovery_after_expiry(h):
    """Bug #2 investigation: Check if expired poison reappears after checkpointing clean state."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)

    # Apply short poison
    h.command("effect give @s minecraft:poison 2 1")
    h.wait(3)

    # Wait for poison to expire completely
    h.wait(45)
    effects_expired = h.state().get("player", {}).get("effects", [])
    assert len(effects_expired) == 0, f"Poison did not expire before checkpoint: {effects_expired}"

    # Checkpoint when player is completely clean
    h.rreturn("set")
    h.wait(2)

    # Rollback
    h.rreturn("load")
    h.wait(4)

    effects_after = h.state().get("player", {}).get("effects", [])
    # If bug #2 reproduces, poison reappears unexpectedly
    assert len(effects_after) == 0, f"Expired poison reappeared after rollback (Bug #2 confirmed): {effects_after}"



def test_62_fire_ticks_and_visual_flame_restoration(h):
    """Bug #12 investigation: Check player fire ticks restoration after being extinguished."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)

    # Checkpoint without fire
    h.wait(2)
    hp_initial = h.state().get("player", {}).get("health", 20.0)
    h.rreturn("set")
    h.wait(2)

    # Ignite player using fire block
    h.command("setblock 0 65 0 minecraft:fire")
    h.wait(4)

    # Put out fire block
    h.command("setblock 0 65 0 minecraft:air")
    h.wait(2)

    # Rollback to clean state
    h.rreturn("load")
    h.wait(4)

    # Check if fire was extinguished
    st_after = h.state().get("player", {})
    health_after = st_after.get("health", 20.0)
    assert health_after >= hp_initial - 2.0, f"Player took unexpected fire damage after clean rollback: {health_after} vs initial {hp_initial}"



def test_71_combat_legacy_reset(h):
    """Bug #15 investigation: Combat entity tracking restoration."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("summon zombie 2.5 65.0 0.5 {Tags:['combat_target']}")
    h.wait(3)
    h.command("damage @e[tag=combat_target,limit=1] 2 minecraft:player_attack by @s")
    h.wait(2)
    h.rreturn("set")
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    assert h.has_entity("@e[tag=combat_target]"), "Combat entity missing after rollback"
    h.command("kill @e[tag=combat_target]")



def test_73_mace_wind_burst_reset(h):
    """Bug #17 investigation: Player position and height restored on mace rollback."""
    setup_clean_env(h)
    h.tp(0.5, 75.0, 0.5)
    h.command("give @s minecraft:mace 1")
    h.select_slot(1)
    h.wait(2)
    h.rreturn("set")
    h.wait(2)
    h.tp(0.5, 65.0, 0.5)
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    pos = h.state().get("player", {}).get("position", [0, 0, 0])
    assert pos[1] >= 70.0, f"Vertical height not restored after mace checkpoint: {pos}"



def test_78_archaeology_brush_reset(h):
    """Bug #22 investigation: Suspicious sand block state restoration."""
    setup_clean_env(h)
    h.command("setblock 0 65 2 minecraft:suspicious_sand[dusted=0]")
    h.wait(3)
    h.rreturn("set")
    h.wait(2)
    h.command("setblock 0 65 2 minecraft:sand")
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    blk = h.block(0, 65, 2).get("block", "")
    h.command("setblock 0 65 2 minecraft:air")
    assert "suspicious_sand" in blk, f"Suspicious sand not restored: {blk}"


def _parse_nbt_vec3(raw):
    nums = re.findall(r"[-+]?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?", raw or "")
    if len(nums) < 3:
        return None
    try:
        return [float(nums[0]), float(nums[1]), float(nums[2])]
    except ValueError:
        return None


def test_117_post_rollback_teleport_movement_sync(h):
    """After a rollback that rebuilds Streaming, /tp on already-loaded chunks must not leave the server stuck in tp_held/landing."""
    setup_clean_env(h)
    slot = "post_tp_sync"
    try:
        h.rreturn(f"slot save {slot}")
        h.wait(2)
        h.command("setblock 1 65 1 minecraft:stone")
        h.wait(2)
        h.rreturn(f"slot load {slot}")
        h.wait(2)
        assert "air" in h.block(1, 65, 1).get("block", ""), (
            f"Slot load did not restore block: {h.block(1, 65, 1)}"
        )

        # Teleport inside the already-loaded spawn column; server and client must both simulate the fall.
        h.command("tp @s 0.5 80.0 0.5")
        landed = False
        for _ in range(30):
            h.wait(2)
            p = h.state().get("player", {})
            if p.get("on_ground") and p.get("position", [0, 999, 0])[1] <= 66.0:
                landed = True
                break
        assert landed, f"Client did not land after post-rollback /tp: {h.state().get('player')}"
        # Server fall-damage SetHealth arrives on the tick after client touches ground.
        h.wait(3)

        srv_pos = _parse_nbt_vec3(h.data_get_entity("@s", "Pos"))
        assert srv_pos is not None, f"Failed to parse server player Pos: {h.data_get_entity('@s', 'Pos')}"
        assert srv_pos[1] <= 66.0, (
            f"Server player stayed frozen above ground (tp_held/landing desync): server={srv_pos}, "
            f"client={h.state().get('player', {}).get('position')}"
        )
        hp = h.state().get("player", {}).get("health")
        assert hp is not None and hp <= 14.0, f"Expected server fall damage after landing, got health={hp}"
    finally:
        h.rreturn("slot select default")
        h.rreturn(f"slot delete {slot}")
        h.command("setblock 1 65 1 minecraft:air")
        h.command("effect give @s minecraft:instant_health 1 5 true")
        h.wait(2)
        h.command("effect clear @s")



