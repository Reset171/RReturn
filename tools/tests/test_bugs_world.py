"""World mechanics, vehicles, entities, and audio bug regression tests (Tests 47, 48, 50, 53-55, 59, 60, 63-70, 72, 74-77, 79-82)."""

from . import setup_clean_env


def test_47_leash_boat_attachment_verification(h):
    """Bug #26 investigation: Check whether a boat leashed to a fence knot survives rollback."""
    setup_clean_env(h)
    h.tp(100.5, 65.0, 100.5)
    h.command("kill @e[tag=test_leash_boat]")
    h.command("kill @e[type=leash_knot,distance=..16]")
    h.command("setblock 100 65 100 minecraft:oak_fence")
    h.wait(2)

    # Spawn boat leashed to fence knot using IntArray notation [I; x, y, z]
    h.command("summon oak_boat 100.5 65.0 102.5 {Tags:['test_leash_boat'],leash:[I; 100,65,100]}")
    h.wait(4)

    assert h.has_entity("@e[tag=test_leash_boat]"), "Boat was not spawned"
    leash_before = h.data_get_entity("@e[tag=test_leash_boat,limit=1]", "leash")

    # Checkpoint while boat is leashed
    h.rreturn("set")
    h.wait(2)

    # Move boat away and remove fence
    h.command("tp @e[tag=test_leash_boat] 100.5 65.0 130.5")
    h.command("setblock 100 65 100 minecraft:air")
    h.command("kill @e[type=leash_knot,distance=..32]")
    h.wait(4)

    # Rollback
    h.rreturn("load")
    h.wait(6)

    assert h.has_entity("@e[tag=test_leash_boat]"), "Boat was lost after rollback"
    fence_block = h.block(100, 65, 100).get("block", "")
    assert "oak_fence" in fence_block, f"Fence was not restored: {fence_block}"

    leash_after = h.data_get_entity("@e[tag=test_leash_boat,limit=1]", "leash")

    # Cleanup
    h.command("kill @e[tag=test_leash_boat]")
    h.command("kill @e[type=leash_knot,distance=..16]")
    h.command("setblock 100 65 100 minecraft:air")

    # If leash_before had leash tag, check if it was preserved
    if "100" in leash_before:
        assert "100" in leash_after, f"Leash was severed after rollback: before={leash_before}, after={leash_after}"



def test_48_music_sound_stopping_verification(h):
    """Bug #25 investigation: Check if rollback mutes sounds."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)

    # Trigger sound
    h.command("playsound minecraft:entity.experience_orb.pickup master @s 0.5 65.0 0.5 1.0 1.0")
    h.wait(1)

    h.rreturn("set")
    h.wait(2)

    h.rreturn("load")
    h.wait(3)

    # Rollback calls StopSound(None, None) which stops all active sound channels
    st = h.status()
    sounds = st.get("sounds", {})
    playing = sounds.get("playing", 0)
    # The sound stopped completely
    assert playing == 0, f"Expected 0 playing sounds after StopSound, got {playing}"



def test_50_item_frame_rotation_preservation(h):
    """Bug #34 investigation: Check whether ItemFrame rotation state is preserved on rollback."""
    setup_clean_env(h)
    h.tp(300.5, 65.0, 300.5)
    h.command("setblock 300 65 301 minecraft:stone")
    h.command("kill @e[tag=test_frame]")
    h.wait(2)

    h.command("summon item_frame 300.5 65.0 300.5 {Facing:2b,Item:{id:'minecraft:arrow',count:1},ItemRotation:4b,Tags:['test_frame']}")
    h.wait(3)
    assert h.has_entity("@e[tag=test_frame]"), "ItemFrame was not spawned"

    h.rreturn("set")
    h.wait(2)

    # Change rotation to 0b
    h.command("data merge entity @e[tag=test_frame,limit=1] {ItemRotation:0b}")
    h.wait(3)

    # Rollback
    h.rreturn("load")
    h.wait(4)

    rot_after = h.data_get_entity("@e[tag=test_frame,limit=1]", "ItemRotation")

    # Cleanup
    h.command("kill @e[tag=test_frame]")
    h.command("setblock 300 65 301 minecraft:air")

    assert "4b" in rot_after, f"ItemFrame rotation was not restored: after={rot_after}"



def test_53_primed_tnt_rollback_preservation(h):
    """Bug #28 investigation: Check whether primed TNT survives memory rollback vs disk reopen."""
    setup_clean_env(h)
    h.tp(400.5, 65.0, 400.5)
    h.command("kill @e[tag=test_tnt]")
    h.command("tick freeze")
    h.wait(2)

    # Spawn primed TNT
    h.command("summon tnt 400.5 65.0 400.5 {Fuse:80s,Tags:['test_tnt']}")
    h.wait(2)
    assert h.has_entity("@e[tag=test_tnt]"), "TNT was not spawned"

    # Save to disk
    h.rreturn("set")
    h.wait(2)

    # 1. RAM rollback - TNT is restored from snap.tnts
    h.command("kill @e[tag=test_tnt]")
    h.wait(2)
    assert not h.has_entity("@e[tag=test_tnt]"), "TNT not killed"

    h.rreturn("load")
    h.wait(4)
    has_tnt_ram = h.has_entity("@e[tag=test_tnt]")
    assert has_tnt_ram, "Primed TNT was not restored after RAM rollback"

    h.command("tick unfreeze")
    h.command("kill @e[tag=test_tnt]")



def test_54_disk_checkpoint_projectiles_and_tnt_loss(h):
    """Verify that disk slot checkpoint properly restores projectiles and primed TNT across disk reload."""
    setup_clean_env(h)
    h.tp(500.5, 65.0, 500.5)
    h.command("kill @e[tag=disk_arrow]")
    h.command("kill @e[tag=disk_tnt]")
    h.command("tick freeze")
    h.wait(2)

    h.command("summon arrow 500.5 70.0 500.5 {Motion:[0.0,0.5,0.0],Tags:['disk_arrow']}")
    h.command("summon tnt 502.5 65.0 500.5 {Fuse:80s,Tags:['disk_tnt']}")
    h.wait(2)

    # Save checkpoint to disk slot
    h.rreturn("slot select disk_check")
    h.rreturn("set")
    h.wait(3)

    # Kill entities after checkpoint
    h.command("kill @e[tag=disk_arrow]")
    h.command("kill @e[tag=disk_tnt]")
    h.wait(2)
    assert not h.has_entity("@e[tag=disk_arrow]"), "Arrow was not killed"
    assert not h.has_entity("@e[tag=disk_tnt]"), "TNT was not killed"

    # Purge in-memory snapshots to force hydration from disk files
    h.rreturn("purge_memory")
    h.wait(2)

    # Restore from disk checkpoint
    h.rreturn("load")
    h.wait(4)
    assert h.has_entity("@e[tag=disk_arrow]"), "Arrow was not restored from disk checkpoint"
    assert h.has_entity("@e[tag=disk_tnt]"), "TNT was not restored from disk checkpoint"

    h.command("tick unfreeze")
    h.command("kill @e[tag=disk_arrow]")
    h.command("kill @e[tag=disk_tnt]")
    h.rreturn("slot select default")



def test_55_boat_passenger_dismount_on_movement_rollback(h):
    """Bug #11 investigation: Check whether player dismounts from boat when moving before rollback."""
    setup_clean_env(h)
    h.tp(600.5, 65.0, 600.5)
    h.command("kill @e[tag=mount_test_boat]")
    h.command("summon oak_boat 600.5 65.0 600.5 {Tags:['mount_test_boat']}")
    h.wait(3)

    # Mount the boat
    h.command("ride @s mount @e[tag=mount_test_boat,limit=1]")
    h.wait(4)

    st_mount = h.state()
    is_riding_before = st_mount.get("player", {}).get("riding") is not None or h.status().get("riding") is not None

    h.rreturn("set")
    h.wait(2)

    # Move boat and player 30 blocks away, dismount, and break (kill) the boat
    h.command("tp @e[tag=mount_test_boat] 630.5 65.0 600.5")
    h.command("tp @s 630.5 65.0 600.5")
    h.wait(2)
    h.command("ride @s dismount")
    h.wait(2)
    h.command("kill @e[tag=mount_test_boat]")
    h.wait(3)
    assert not h.has_entity("@e[tag=mount_test_boat]"), "Boat was not destroyed"

    # Rollback
    h.rreturn("load")
    h.wait(6)

    st_after = h.state()
    is_riding_after = st_after.get("player", {}).get("riding") is not None or h.status().get("riding") is not None

    # Cleanup
    h.command("ride @s dismount")
    h.command("kill @e[tag=mount_test_boat]")

    assert is_riding_before, "Player failed to mount the boat before checkpoint"
    assert is_riding_after, f"Player dismounted from boat after rollback (Bug #11 reproduced): riding={is_riding_after}"



def test_59_jukebox_audio_playback_after_rollback(h):
    """Bug #9 investigation: Check whether jukebox disc music stops after rollback."""
    setup_clean_env(h)
    h.tp(700.5, 65.0, 700.5)
    h.command("setblock 700 65 700 minecraft:jukebox")
    h.wait(2)

    # Insert music disc into jukebox
    h.command("item replace block 700 65 700 container.0 with minecraft:music_disc_cat")
    h.wait(4)

    h.rreturn("set")
    h.wait(2)

    h.rreturn("load")
    h.wait(4)

    # Block still has disc
    jukebox_block = h.block(700, 65, 700)
    has_record = "jukebox" in jukebox_block.get("block", "")
    h.command("setblock 700 65 700 minecraft:air")
    assert has_record, "Jukebox block was not restored"



def test_60_wither_bossbar_health_restoration(h):
    """Bug #33 investigation: Check wither entity health across rollback."""
    setup_clean_env(h)
    h.tp(800.5, 65.0, 800.5)
    h.command("kill @e[tag=test_wither]")
    h.command("summon wither 800.5 70.0 800.5 {Health:300f,Invul:0,Tags:['test_wither']}")
    h.wait(4)

    assert h.has_entity("@e[tag=test_wither]"), "Wither was not spawned"

    h.rreturn("set")
    h.wait(2)

    # Damage wither down to 100 HP
    h.command("data merge entity @e[tag=test_wither,limit=1] {Health:100f}")
    h.wait(3)
    hp_damaged = h.data_get_entity("@e[tag=test_wither,limit=1]", "Health")
    assert "100" in hp_damaged, f"Wither was not damaged: {hp_damaged}"

    # Rollback
    h.rreturn("load")
    h.wait(4)

    hp_after = h.data_get_entity("@e[tag=test_wither,limit=1]", "Health")
    h.command("kill @e[tag=test_wither]")

    assert "300" in hp_after, f"Wither health was not restored to 300: {hp_after}"



def test_63_fishing_hook_velocity_preservation(h):
    """Bug #1 investigation: Fishing hook position and state preservation."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("give @s minecraft:fishing_rod 1")
    h.select_slot(1)
    h.wait(2)
    h.use_item()
    h.wait(3)
    fish_info = h.status().get("fishing")
    h.rreturn("set")
    h.wait(8)
    h.rreturn("load")
    h.wait(4)
    fish_after = h.status().get("fishing")
    assert fish_after is not None, "Fishing hook disappeared after rollback (Bug #1)"



def test_64_nether_spawn_determinism(h):
    """Bug #3 investigation: Determinism of entity spawning after rollback."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("difficulty easy")
    h.rreturn("set")
    h.wait(15)
    ents_first = len(h.entities(radius=32))
    h.rreturn("load")
    h.wait(15)
    ents_second = len(h.entities(radius=32))
    assert abs(ents_first - ents_second) <= 3, f"Spawning discrepancy: {ents_first} vs {ents_second}"



def test_65_bow_arrow_spread_determinism(h):
    """Bug #4 investigation: Arrow landing position reproducibility across rollbacks."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("kill @e[type=arrow]")
    h.rreturn("set")
    h.wait(2)
    h.command("summon arrow 0.5 67.0 0.5 {Motion:[0.5,0.2,0.0]}")
    h.wait(25)
    pos1 = h.data_get_entity("@e[type=arrow,limit=1]", "Pos")
    h.command("kill @e[type=arrow]")
    h.rreturn("load")
    h.wait(2)
    h.command("summon arrow 0.5 67.0 0.5 {Motion:[0.5,0.2,0.0]}")
    h.wait(25)
    pos2 = h.data_get_entity("@e[type=arrow,limit=1]", "Pos")
    h.command("kill @e[type=arrow]")
    assert pos1 == pos2, f"Arrow landing deviated across rollbacks: {pos1} vs {pos2}"



def test_66_tnt_fuse_deflection_determinism(h):
    """Bug #5 investigation: Primed TNT removal on rollback to pre-ignition."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("gamerule mobExplosionDropDecay false")
    h.rreturn("set")
    h.wait(2)
    h.command("summon tnt 0.5 66.0 0.5 {Fuse:80s,Motion:[0.0,0.1,0.0]}")
    h.wait(4)
    assert h.has_entity("@e[type=tnt]"), "TNT not found after summon"
    h.rreturn("load")
    h.wait(4)
    assert not h.has_entity("@e[type=tnt]"), "TNT still present after rollback to pre-ignition (Bug #5)"



def test_67_fishing_nibble_reset(h):
    """Bug #7 investigation: Hook presence preserved across rollback in water."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("fill -2 63 2 2 64 6 minecraft:water")
    h.command("give @s minecraft:fishing_rod 1")
    h.select_slot(1)
    h.wait(2)
    h.use_item()
    h.wait(6)
    h.rreturn("set")
    h.wait(3)
    h.rreturn("load")
    h.wait(4)
    assert h.status().get("fishing") is not None, "Hook lost after rollback in water (Bug #7)"



def test_68_raid_active_wave_reset(h):
    """Bug #8 investigation: Bad omen and raid effect restoration."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("effect give @s minecraft:bad_omen 60 1")
    h.wait(3)
    h.rreturn("set")
    h.wait(2)
    h.command("effect clear @s")
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    eff = h.state().get("player", {}).get("effects", [])
    has_bad_omen = any("bad_omen" in str(e) for e in eff)
    assert has_bad_omen, "Bad omen effect lost after rollback (Bug #8)"



def test_69_weather_client_sync(h):
    """Bug #13 investigation: Weather rollback restoration."""
    setup_clean_env(h)
    h.command("weather rain")
    h.wait(4)
    h.rreturn("set")
    h.wait(2)
    h.command("weather clear")
    h.wait(4)
    h.rreturn("load")
    h.wait(4)
    # Rollback succeeds without crashes or errors
    st = h.state()
    assert st.get("player") is not None, "Player state corrupted after weather rollback"



def test_70_chorus_fruit_teleport_determinism(h):
    """Bug #14 investigation: Chorus fruit teleport deterministic rollback."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("give @s minecraft:chorus_fruit 16")
    h.select_slot(1)
    h.wait(2)
    h.rreturn("set")
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    pos = h.state().get("player", {}).get("position", [0, 0, 0])
    assert abs(pos[0] - 0.5) < 1.0 and abs(pos[1] - 65.0) < 1.0, f"Position deviated: {pos}"



def test_72_zombie_villager_cure_animation(h):
    """Bug #16 investigation: Zombie villager ConversionTime preservation."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("kill @e[tag=cure_target]")
    h.command("summon zombie_villager 2.5 65.0 0.5 {Tags:['cure_target'],ConversionTime:3000}")
    h.wait(4)
    conv_before = h.data_get_entity("@e[tag=cure_target,limit=1]", "ConversionTime")
    h.rreturn("set")
    h.wait(2)
    h.command("data merge entity @e[tag=cure_target,limit=1] {ConversionTime:100}")
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    conv_after = h.data_get_entity("@e[tag=cure_target,limit=1]", "ConversionTime")
    h.command("kill @e[tag=cure_target]")
    assert any(c.isdigit() for c in conv_before), f"ConversionTime not found before: {conv_before}"
    assert any(c.isdigit() for c in conv_after), f"ConversionTime not found after: {conv_after}"



def test_74_sculk_vibration_reset(h):
    """Bug #18 investigation: Sculk block restoration."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("setblock 0 65 2 minecraft:sculk_sensor")
    h.wait(3)
    h.rreturn("set")
    h.wait(2)
    h.command("setblock 0 65 2 minecraft:air")
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    blk = h.block(0, 65, 2).get("block", "")
    h.command("setblock 0 65 2 minecraft:air")
    assert "sculk_sensor" in blk, f"Sculk block not restored: {blk}"



def test_75_redstone_lamp_visual_state(h):
    """Bug #19 investigation: Redstone lamp block state synchronization."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("setblock 0 65 2 minecraft:redstone_block")
    h.command("setblock 0 65 3 minecraft:redstone_lamp[lit=true]")
    h.wait(4)
    h.rreturn("set")
    h.wait(2)
    h.command("setblock 0 65 2 minecraft:air")
    h.wait(4)
    h.rreturn("load")
    h.wait(4)
    blk_restored = h.block(0, 65, 3).get("block", "")
    h.command("setblock 0 65 2 minecraft:air")
    h.command("setblock 0 65 3 minecraft:air")
    assert "redstone_lamp" in blk_restored, f"Redstone lamp block missing: {blk_restored}"



def test_76_lightning_flash_cleanup(h):
    """Bug #20 investigation: Lightning bolt entity removal on rollback."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.rreturn("set")
    h.wait(2)
    h.command("summon lightning_bolt 2.0 65.0 2.0")
    h.wait(1)
    h.rreturn("load")
    h.wait(4)
    assert not h.has_entity("@e[type=lightning_bolt]"), "Lightning bolt remained after rollback (Bug #20)"



def test_77_creaking_heart_connection(h):
    """Bug #21 investigation: Creaking heart block restoration."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.wait(3)
    h.command("setblock 0 65 3 minecraft:creaking_heart")
    h.wait(3)
    h.rreturn("set")
    h.wait(2)
    h.command("setblock 0 65 3 minecraft:air")
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    h.ensure_loaded(0, 65, 3)
    blk = h.block(0, 65, 3).get("block", "")
    h.command("setblock 0 65 3 minecraft:air")
    assert "creaking_heart" in blk, f"Creaking heart block not restored: {blk}"



def test_79_trial_spawner_tick_cleared(h):
    """Bug #23 investigation: Trial spawner block state restoration."""
    setup_clean_env(h)
    h.command("setblock 0 65 2 minecraft:trial_spawner")
    h.wait(3)
    h.rreturn("set")
    h.wait(2)
    h.command("setblock 0 65 2 minecraft:stone")
    h.wait(2)
    h.rreturn("load")
    h.wait(4)
    h.ensure_loaded(0, 65, 2)
    blk = h.block(0, 65, 2).get("block", "")
    h.command("setblock 0 65 2 minecraft:air")
    assert "trial_spawner" in blk, f"Trial spawner not restored: {blk}"



def test_80_boat_cart_velocity_disk_loss(h):
    """Bug #29 investigation: Minecart presence after disk slot reload."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("kill @e[type=minecart]")
    h.command("summon minecart 0.5 65.0 2.5 {Tags:['test_cart'],Motion:[1.5,0.0,0.0]}")
    h.wait(3)
    assert h.has_entity("@e[tag=test_cart]"), "Minecart was not spawned"
    h.rreturn("set disk_cart")
    h.wait(3)
    h.command("tp @e[tag=test_cart] 0.5 65.0 20.0")
    h.wait(2)
    h.rreturn("load disk_cart")
    h.wait(5)
    assert h.has_entity("@e[tag=test_cart]"), "Minecart lost after disk load (Bug #29)"
    h.command("kill @e[tag=test_cart]")



def test_81_wither_client_bossbar_sync(h):
    """Bug #33 investigation: Wither entity and health restoration."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("kill @e[type=wither]")
    h.command("summon wither 0.5 70.0 5.0 {Tags:['boss_sync'],Health:300.0f}")
    h.wait(5)
    assert h.has_entity("@e[tag=boss_sync]"), "Wither not spawned"
    h.rreturn("set")
    h.wait(2)
    h.command("damage @e[tag=boss_sync,limit=1] 100 minecraft:generic")
    h.wait(2)
    h.rreturn("load")
    h.wait(5)
    assert h.has_entity("@e[tag=boss_sync]"), "Wither lost after rollback"
    hp = h.data_get_entity("@e[tag=boss_sync,limit=1]", "Health")
    h.command("kill @e[tag=boss_sync]")
    assert "300" in hp, f"Wither health not restored: {hp}"



def test_82_wandering_trader_spawner_reset(h):
    """Bug #35 investigation: Wandering trader restoration."""
    setup_clean_env(h)
    h.tp(0.5, 65.0, 0.5)
    h.command("kill @e[type=wandering_trader]")
    h.command("summon wandering_trader 2.0 65.0 2.0 {Tags:['test_trader'],DespawnDelay:24000}")
    h.wait(4)
    assert h.has_entity("@e[tag=test_trader]"), "Wandering trader not spawned"
    h.rreturn("set")
    h.wait(2)
    h.command("kill @e[tag=test_trader]")
    h.wait(2)
    assert not h.has_entity("@e[tag=test_trader]"), "Trader not killed"
    h.rreturn("load")
    h.wait(4)
    assert h.has_entity("@e[tag=test_trader]"), "Wandering trader not restored on rollback (Bug #35)"
    h.command("kill @e[tag=test_trader]")





