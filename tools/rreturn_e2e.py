import argparse
from concurrent.futures import ThreadPoolExecutor
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from tools.rreturn_harness import RReturnHarness, purge_test_worlds_on_disk
from tools.tests.test_chunks import test_1_unloaded_chunks
from tools.tests.test_explosions import (
    test_2_tnt_explosion_rollback,
    test_3_tnt_future_chain_isolation,
)
from tools.tests.test_vehicles import (
    test_4_fishing_bobber_preservation,
    test_5_boat_mount_rollback,
    test_16_flying_arrow_projectile,
    test_45_fishing_bobber_unheld_rod_rollback,
)
from tools.tests.test_mobs_ai import (
    test_6_remote_chunks_mob_cleanup,
    test_7_villager_ai_determinism,
    test_8_spawner_determinism,
    test_42_sheep_sheared_and_color_rollback,
)
from tools.tests.test_death_rewind import (
    test_9_return_by_death_fatal_fall,
    test_10_multiple_fall_cycles,
    test_18_drowning_and_damage_after_rewind,
    test_43_potion_effects_cleared_on_rollback,
)
from tools.tests.test_slots import (
    test_11_slots_branching,
    test_41_disk_checkpoint_reopen_and_rollback,
    test_44_disk_checkpoint_singleplayer_uuid_fallback,
)
from tools.tests.test_policies import test_12_policy_selective_inventory
from tools.tests.test_containers import (
    test_14_chest_container_rollback,
    test_15_piston_redstone_machine,
)
from tools.tests.test_dimensions import test_17_end_dragon_bossbar_rollback
from tools.tests.test_ai_telemetry import test_13_ai_json_telemetry
from tools.tests.test_world_settings import (
    test_19_tick_rate_and_freeze_rollback,
    test_20_world_spawn_and_difficulty_lock,
    test_21_schedule_function_cancellation,
    test_22_fillbiome_rollback,
    test_23_weather_and_thunder_rollback,
)
from tools.tests.test_portals import (
    test_24_nether_portal_cold_chunk,
    test_25_flying_ender_pearl_discard,
    test_26_portal_processor_cooldown,
    test_27_end_credits_respawn_safety,
    test_46_nether_portal_creative_rollback_and_reenter,
)
from tools.tests.test_mobs_advanced import (
    test_28_shoulder_parrots_rollback,
    test_29_neutral_mob_anger_reset,
    test_30_leash_distance_preservation,
    test_31_slime_jump_delay_determinism,
    test_32_sleeping_villager_day_schedule,
)
from tools.tests.test_containers_advanced import (
    test_33_furnace_cook_and_burn_time,
    test_34_hopper_transfer_cooldown,
    test_35_item_frame_duplication_safety,
    test_36_creative_cursor_item_retention,
)
from tools.tests.test_determinism_advanced import (
    test_37_raid_determinism_and_coords,
    test_38_chest_loot_table_determinism,
    test_39_explored_map_fog_of_war,
    test_40_subchunk_palette_expansion_bounds,
)
from tools.tests.test_bugs_investigation import (
    test_47_leash_boat_attachment_verification,
    test_48_music_sound_stopping_verification,
    test_49_thrown_potion_cloud_preservation,
    test_50_item_frame_rotation_preservation,
    test_51_powder_snow_ticks_frozen_preservation,
    test_52_gui_closure_verification,
    test_53_primed_tnt_rollback_preservation,
    test_54_disk_checkpoint_projectiles_and_tnt_loss,
    test_55_boat_passenger_dismount_on_movement_rollback,
    test_56_singleplayer_shield_cooldown_restoration,
    test_57_fall_flying_elytra_mode_restoration,
    test_58_using_item_interruption_on_rollback,
    test_59_jukebox_audio_playback_after_rollback,
    test_60_wither_bossbar_health_restoration,
    test_61_poison_effect_recovery_after_expiry,
    test_62_fire_ticks_and_visual_flame_restoration,
    test_63_fishing_hook_velocity_preservation,
    test_64_nether_spawn_determinism,
    test_65_bow_arrow_spread_determinism,
    test_66_tnt_fuse_deflection_determinism,
    test_67_fishing_nibble_reset,
    test_68_raid_active_wave_reset,
    test_69_weather_client_sync,
    test_70_chorus_fruit_teleport_determinism,
    test_71_combat_legacy_reset,
    test_72_zombie_villager_cure_animation,
    test_73_mace_wind_burst_reset,
    test_74_sculk_vibration_reset,
    test_75_redstone_lamp_visual_state,
    test_76_lightning_flash_cleanup,
    test_77_creaking_heart_connection,
    test_78_archaeology_brush_reset,
    test_79_trial_spawner_tick_cleared,
    test_80_boat_cart_velocity_disk_loss,
    test_81_wither_client_bossbar_sync,
    test_82_wandering_trader_spawner_reset,
)
from tools.tests.test_scrubbing import (
    test_83_interactive_time_scrubbing_and_rewind,
    test_84_scrub_mob_motion_freeze_and_no_extrapolation,
    test_85_scrub_fov_bobbing_and_hurt_overlay,
    test_86_scrub_player_inventory_health_and_effects_no_dupe,
    test_87_scrub_mob_resurrection_and_tnt_explosion_revert,
)
from tools.tests.test_scrubbing_advanced import (
    test_88_scrub_continuous_mob_motion_no_stutter,
    test_89_scrub_armor_and_equipment_rollback,
    test_90_scrub_experience_level_and_progress_rollback,
    test_91_scrub_dropped_item_pickup_and_reemergence,
    test_92_scrub_fire_ticks_and_extinguish_rollback,
    test_93_scrub_rapid_toggle_and_cancel_stability,
    test_94_scrub_container_furnace_cook_time_rollback,
    test_95_scrub_redstone_lamp_power_toggle_rollback,
    test_96_scrub_mass_block_fill_and_restore,
    test_97_scrub_weather_rain_and_thunder_rollback,
    test_98_scrub_flying_arrow_trajectory_rollback,
    test_99_scrub_cursor_bounds_clamping,
    test_100_scrub_rewind_subcommand_full_cycle,
    test_101_scrub_dispenser_prng_selection_determinism,
    test_102_scrub_chest_loot_table_seed_determinism,
    test_103_scrub_tnt_fuse_and_deterministic_deflection,
    test_104_scrub_villager_ai_schedule_determinism,
)

HEAVY_WORLD_GROUPS = {"chunks", "dimensions", "portals"}


class TestRunner:
    def __init__(self, harness, worker_label=""):
        self.h = harness
        self.worker_label = f"[{worker_label}] " if worker_label else ""
        self.passed = 0
        self.failed = 0
        self.results = []

    def log(self, test_id, name, status, duration, details=""):
        res = {
            "id": test_id,
            "name": name,
            "status": status,
            "duration_ms": int(duration * 1000),
            "details": details,
        }
        self.results.append(res)
        color = "\033[92m[PASS]\033[0m" if status == "PASS" else "\033[91m[FAIL]\033[0m"
        print(f" {self.worker_label}{color} {name} ({res['duration_ms']} ms) {details}", flush=True)
        if status == "PASS":
            self.passed += 1
        else:
            self.failed += 1

    def run_test(self, test_id, name, func):
        self.h.current_test_id = test_id
        print(f"\n--- {self.worker_label}Running: {name} ---", flush=True)
        start = time.time()
        try:
            func(self.h)
            self.log(test_id, name, "PASS", time.time() - start)
        except Exception as e:
            self.log(test_id, name, "FAIL", time.time() - start, str(e))


def build_test_catalog():
    return [
        (1, "Test 1: Unloaded Chunks Isolation", "chunks", test_1_unloaded_chunks),
        (2, "Test 2: Massive TNT Explosion Rollback", "explosions", test_2_tnt_explosion_rollback),
        (3, "Test 3: TNT Future Ignition Chain Isolation", "explosions", test_3_tnt_future_chain_isolation),
        (4, "Test 4: Fishing Bobber Preservation", "vehicles", test_4_fishing_bobber_preservation),
        (5, "Test 5: Boat Mount Rollback", "vehicles", test_5_boat_mount_rollback),
        (6, "Test 6: Remote Chunks Mob Cleanup", "mobs", test_6_remote_chunks_mob_cleanup),
        (7, "Test 7: Villager AI Determinism", "mobs", test_7_villager_ai_determinism),
        (8, "Test 8: Spawner Determinism", "mobs", test_8_spawner_determinism),
        (9, "Test 9: Return by Death Fatal Fall", "death", test_9_return_by_death_fatal_fall),
        (10, "Test 10: Multiple Fall Cycles Anti-Invulnerability", "death", test_10_multiple_fall_cycles),
        (11, "Test 11: Timeline Branching with Named Slots", "slots", test_11_slots_branching),
        (12, "Test 12: Selective Rollback Policy", "policies", test_12_policy_selective_inventory),
        (13, "Test 13: Machine-Readable AI Telemetry JSON", "ai", test_13_ai_json_telemetry),
        (14, "Test 14: Chest Container Inventory Rollback", "containers", test_14_chest_container_rollback),
        (15, "Test 15: Piston and Redstone State Rollback", "containers", test_15_piston_redstone_machine),
        (16, "Test 16: Flying Projectile Arrow Determinism", "vehicles", test_16_flying_arrow_projectile),
        (17, "Test 17: End Dragon Bossbar Cross-Dimension Rollback", "dimensions", test_17_end_dragon_bossbar_rollback),
        (18, "Test 18: Drowning and Damage After Rewind", "death", test_18_drowning_and_damage_after_rewind),
        (19, "Test 19: Tick Rate and Freeze Rollback", "world", test_19_tick_rate_and_freeze_rollback),
        (20, "Test 20: World Spawn and Difficulty Lock", "world", test_20_world_spawn_and_difficulty_lock),
        (21, "Test 21: Scheduled Function Cancellation", "world", test_21_schedule_function_cancellation),
        (22, "Test 22: Biome Palette FillBiome Rollback", "world", test_22_fillbiome_rollback),
        (23, "Test 23: Weather and Thunder State Rollback", "world", test_23_weather_and_thunder_rollback),
        (24, "Test 24: Nether Portal Cold Chunk Disappearance Fix", "portals", test_24_nether_portal_cold_chunk),
        (25, "Test 25: Flying Ender Pearl Discard", "portals", test_25_flying_ender_pearl_discard),
        (26, "Test 26: Portal Processor Cooldown Reset", "portals", test_26_portal_processor_cooldown),
        (27, "Test 27: End Credits Respawn Safety", "portals", test_27_end_credits_respawn_safety),
        (28, "Test 28: Shoulder Parrots Rollback", "mobs", test_28_shoulder_parrots_rollback),
        (29, "Test 29: Neutral Mob Anger Reset", "mobs", test_29_neutral_mob_anger_reset),
        (30, "Test 30: Leash Distance Preservation", "mobs", test_30_leash_distance_preservation),
        (31, "Test 31: Slime Jump Delay Determinism", "mobs", test_31_slime_jump_delay_determinism),
        (32, "Test 32: Sleeping Villager Day Schedule Sync", "mobs", test_32_sleeping_villager_day_schedule),
        (33, "Test 33: Furnace Cook and Burn Time Rollback", "containers", test_33_furnace_cook_and_burn_time),
        (34, "Test 34: Hopper Transfer Cooldown Preservation", "containers", test_34_hopper_transfer_cooldown),
        (35, "Test 35: Item Frame and ArmorStand Duplication Safety", "containers", test_35_item_frame_duplication_safety),
        (36, "Test 36: Player Cursor Inventory Retention", "containers", test_36_creative_cursor_item_retention),
        (37, "Test 37: Raid Bad Omen Determinism", "determinism", test_37_raid_determinism_and_coords),
        (38, "Test 38: Chest Loot Table Preservation", "determinism", test_38_chest_loot_table_determinism),
        (39, "Test 39: Explored Map Fog of War Rollback", "determinism", test_39_explored_map_fog_of_war),
        (40, "Test 40: Subchunk Palette Bounds Y 318 and -50", "determinism", test_40_subchunk_palette_expansion_bounds),
        (41, "Test 41: Disk Checkpoint Reopen and Anti-XRay Rollback", "slots", test_41_disk_checkpoint_reopen_and_rollback),
        (42, "Test 42: Sheep Sheared and Wool Color Rollback", "mobs", test_42_sheep_sheared_and_color_rollback),
        (43, "Test 43: Potion Effects Cleared on Rollback", "death", test_43_potion_effects_cleared_on_rollback),
        (44, "Test 44: Disk Checkpoint Singleplayer UUID Fallback", "slots", test_44_disk_checkpoint_singleplayer_uuid_fallback),
        (45, "Test 45: Fishing Bobber Unheld Rod Rollback", "vehicles", test_45_fishing_bobber_unheld_rod_rollback),
        (46, "Test 46: Nether Portal Creative Rollback and Reenter", "portals", test_46_nether_portal_creative_rollback_and_reenter),
        (47, "Test 47: Leash Boat Attachment Verification", "investigation", test_47_leash_boat_attachment_verification),
        (48, "Test 48: Music and Ambient StopSound Verification", "investigation", test_48_music_sound_stopping_verification),
        (49, "Test 49: Thrown Splash Potion Cloud Preservation", "investigation", test_49_thrown_potion_cloud_preservation),
        (50, "Test 50: Item Frame Rotation Preservation", "investigation", test_50_item_frame_rotation_preservation),
        (51, "Test 51: Powder Snow TicksFrozen Preservation", "investigation", test_51_powder_snow_ticks_frozen_preservation),
        (52, "Test 52: Container and GUI Forced Closure", "investigation", test_52_gui_closure_verification),
        (53, "Test 53: Primed TNT Rollback Preservation", "investigation", test_53_primed_tnt_rollback_preservation),
        (54, "Test 54: Disk Checkpoint Projectiles and TNT Loss", "investigation", test_54_disk_checkpoint_projectiles_and_tnt_loss),
        (55, "Test 55: Boat Passenger Dismount on Movement Rollback", "investigation", test_55_boat_passenger_dismount_on_movement_rollback),
        (56, "Test 56: Singleplayer Shield Cooldown Restoration", "investigation", test_56_singleplayer_shield_cooldown_restoration),
        (57, "Test 57: Fall Flying Elytra Mode Restoration", "investigation", test_57_fall_flying_elytra_mode_restoration),
        (58, "Test 58: Using Item Interruption on Rollback", "investigation", test_58_using_item_interruption_on_rollback),
        (59, "Test 59: Jukebox Audio Playback After Rollback", "investigation", test_59_jukebox_audio_playback_after_rollback),
        (60, "Test 60: Wither BossBar Health Restoration", "investigation", test_60_wither_bossbar_health_restoration),
        (61, "Test 61: Poison Effect Recovery After Expiry", "investigation", test_61_poison_effect_recovery_after_expiry),
        (62, "Test 62: Fire Ticks and Visual Flame Restoration", "investigation", test_62_fire_ticks_and_visual_flame_restoration),
        (63, "Test 63: Fishing Hook Velocity Preservation", "investigation", test_63_fishing_hook_velocity_preservation),
        (64, "Test 64: Nether Spawn Determinism", "investigation", test_64_nether_spawn_determinism),
        (65, "Test 65: Bow Arrow Spread Determinism", "investigation", test_65_bow_arrow_spread_determinism),
        (66, "Test 66: TNT Fuse Deflection Determinism", "investigation", test_66_tnt_fuse_deflection_determinism),
        (67, "Test 67: Fishing Nibble Reset", "investigation", test_67_fishing_nibble_reset),
        (68, "Test 68: Raid Active Wave Reset", "investigation", test_68_raid_active_wave_reset),
        (69, "Test 69: Weather Client Sync", "investigation", test_69_weather_client_sync),
        (70, "Test 70: Chorus Fruit Teleport Determinism", "investigation", test_70_chorus_fruit_teleport_determinism),
        (71, "Test 71: Combat Legacy Reset", "investigation", test_71_combat_legacy_reset),
        (72, "Test 72: Zombie Villager Cure Animation", "investigation", test_72_zombie_villager_cure_animation),
        (73, "Test 73: Mace Wind Burst Reset", "investigation", test_73_mace_wind_burst_reset),
        (74, "Test 74: Sculk Vibration Reset", "investigation", test_74_sculk_vibration_reset),
        (75, "Test 75: Redstone Lamp Visual State", "investigation", test_75_redstone_lamp_visual_state),
        (76, "Test 76: Lightning Flash Cleanup", "investigation", test_76_lightning_flash_cleanup),
        (77, "Test 77: Creaking Heart Connection", "investigation", test_77_creaking_heart_connection),
        (78, "Test 78: Archaeology Brush Reset", "investigation", test_78_archaeology_brush_reset),
        (79, "Test 79: Trial Spawner Tick Cleared", "investigation", test_79_trial_spawner_tick_cleared),
        (80, "Test 80: Boat and Cart Velocity Disk Loss", "investigation", test_80_boat_cart_velocity_disk_loss),
        (81, "Test 81: Wither Client BossBar Sync", "investigation", test_81_wither_client_bossbar_sync),
        (82, "Test 82: Wandering Trader Spawner Reset", "investigation", test_82_wandering_trader_spawner_reset),
        (83, "Test 83: Interactive Time Scrubbing and Rewind", "scrubbing", test_83_interactive_time_scrubbing_and_rewind),
        (84, "Test 84: Scrub Mob Motion Freeze and No Extrapolation", "scrubbing", test_84_scrub_mob_motion_freeze_and_no_extrapolation),
        (85, "Test 85: Scrub FOV Bobbing and Hurt Overlay", "scrubbing", test_85_scrub_fov_bobbing_and_hurt_overlay),
        (86, "Test 86: Scrub Player Inventory, Health, and Effects No Dupe", "scrubbing", test_86_scrub_player_inventory_health_and_effects_no_dupe),
        (87, "Test 87: Scrub Mob Resurrection and TNT Revert", "scrubbing", test_87_scrub_mob_resurrection_and_tnt_explosion_revert),
        (88, "Test 88: Scrub Continuous Mob Motion No Stutter", "scrubbing", test_88_scrub_continuous_mob_motion_no_stutter),
        (89, "Test 89: Scrub Armor and Equipment Rollback", "scrubbing", test_89_scrub_armor_and_equipment_rollback),
        (90, "Test 90: Scrub Experience Level and Progress Rollback", "scrubbing", test_90_scrub_experience_level_and_progress_rollback),
        (91, "Test 91: Scrub Dropped Item Pickup and Reemergence", "scrubbing", test_91_scrub_dropped_item_pickup_and_reemergence),
        (92, "Test 92: Scrub Fire Ticks and Extinguish Rollback", "scrubbing", test_92_scrub_fire_ticks_and_extinguish_rollback),
        (93, "Test 93: Scrub Rapid Toggle and Cancel Stability", "scrubbing", test_93_scrub_rapid_toggle_and_cancel_stability),
        (94, "Test 94: Scrub Container Furnace Cook Time Rollback", "scrubbing", test_94_scrub_container_furnace_cook_time_rollback),
        (95, "Test 95: Scrub Redstone Lamp Power Toggle Rollback", "scrubbing", test_95_scrub_redstone_lamp_power_toggle_rollback),
        (96, "Test 96: Scrub Mass Block Fill and Restore", "scrubbing", test_96_scrub_mass_block_fill_and_restore),
        (97, "Test 97: Scrub Weather Rain and Thunder Rollback", "scrubbing", test_97_scrub_weather_rain_and_thunder_rollback),
        (98, "Test 98: Scrub Flying Arrow Trajectory Rollback", "scrubbing", test_98_scrub_flying_arrow_trajectory_rollback),
        (99, "Test 99: Scrub Cursor Bounds Clamping", "scrubbing", test_99_scrub_cursor_bounds_clamping),
        (100, "Test 100: Scrub Rewind Subcommand Full Cycle", "scrubbing", test_100_scrub_rewind_subcommand_full_cycle),
        (101, "Test 101: Scrub Dispenser PRNG Selection Determinism", "determinism", test_101_scrub_dispenser_prng_selection_determinism),
        (102, "Test 102: Scrub Chest Loot Table Seed Determinism", "determinism", test_102_scrub_chest_loot_table_seed_determinism),
        (103, "Test 103: Scrub TNT Fuse and Deterministic Deflection", "determinism", test_103_scrub_tnt_fuse_and_deterministic_deflection),
        (104, "Test 104: Scrub Villager AI Schedule Determinism", "determinism", test_104_scrub_villager_ai_schedule_determinism),
    ]


def run_worker_batch(worker_idx, batch, use_default_socket=False, fresh_worlds=True, keep_worlds=False):
    stamp = int(time.time())
    base_world = f"rreturn_e2e_w{worker_idx}_{stamp}"
    if use_default_socket:
        h = RReturnHarness(world=base_world)
        label = ""
    else:
        sock = Path(tempfile.gettempdir()) / f"rreturn_w{worker_idx}_{stamp}.sock"
        h = RReturnHarness(socket_path=sock, world=base_world)
        label = f"W{worker_idx}"

    h.connect(timeout=60)
    if fresh_worlds and not h.launched_new_process:
        h.open_world(f"{base_world}_core", seed=12345)
    else:
        h.wait_ready(timeout=60)
    runner = TestRunner(h, worker_label=label)

    try:
        light_tests = [t for t in batch if t[2] not in HEAVY_WORLD_GROUPS]
        heavy_tests = [t for t in batch if t[2] in HEAVY_WORLD_GROUPS]

        if light_tests:
            for test_id, name, _, fn in light_tests:
                runner.run_test(test_id, name, fn)

        if heavy_tests:
            last_group = None
            for idx, (test_id, name, grp, fn) in enumerate(heavy_tests):
                if fresh_worlds and (grp != last_group or (idx == 0 and light_tests)):
                    h.open_world(f"{base_world}_{grp}", seed=12345)
                    last_group = grp
                runner.run_test(test_id, name, fn)
    finally:
        if use_default_socket:
            if fresh_worlds:
                h.cleanup_worlds(keep_worlds=keep_worlds)
            if h.launched_new_process:
                h.stop_instance(keep_worlds=keep_worlds)
            else:
                h.close()
                if fresh_worlds and not keep_worlds:
                    purge_test_worlds_on_disk(h.instance)
        else:
            h.stop_instance(keep_worlds=keep_worlds)
            if h.socket_path.exists():
                h.socket_path.unlink(missing_ok=True)
    return runner


def main():
    parser = argparse.ArgumentParser(description="RReturn E2E modular test suite for Horizon engine")
    parser.add_argument("--test", type=str, default="all", help="Test number or search string")
    parser.add_argument(
        "--group",
        type=str,
        default=None,
        help="Filter by group (chunks, explosions, vehicles, mobs, death, slots, policies, containers, dimensions, ai, world, portals, determinism)",
    )
    parser.add_argument(
        "--fast",
        "--smoke",
        action="store_true",
        help="Run 5 fast representative smoke tests instead of the full suite",
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=1,
        help="Number of parallel game instances to run tests across (default: 1)",
    )
    parser.add_argument(
        "--in-place",
        action="store_true",
        help="Run inside the currently open world without creating fresh temporary worlds",
    )
    parser.add_argument(
        "--keep-worlds",
        action="store_true",
        help="Keep temporary rreturn_e2e_* worlds in saves/ after the run",
    )
    args = parser.parse_args()

    catalog = build_test_catalog()
    selected = catalog
    if args.fast:
        fast_ids = {1, 2, 9, 14, 19}
        selected = [t for t in selected if t[0] in fast_ids]
    elif args.group:
        selected = [t for t in selected if t[2].lower() == args.group.lower()]

    if args.test != "all":
        try:
            num = int(args.test)
            selected = [t for t in selected if t[0] == num]
        except ValueError:
            selected = [t for t in selected if args.test.lower() in t[1].lower()]

    workers = max(1, min(args.workers, len(selected)))
    fresh_worlds = not args.in_place
    print("\n=======================================================")
    print(f"   RReturn Modular E2E Suite ({len(selected)} tests, workers={workers}, fresh_worlds={fresh_worlds})")
    print("=======================================================")

    total_start = time.time()
    if workers == 1:
        runners = [
            run_worker_batch(
                1,
                selected,
                use_default_socket=True,
                fresh_worlds=fresh_worlds,
                keep_worlds=args.keep_worlds,
            )
        ]
    else:
        batches = [[] for _ in range(workers)]
        for idx, item in enumerate(selected):
            batches[idx % workers].append(item)
        with ThreadPoolExecutor(max_workers=workers) as pool:
            futures = [
                pool.submit(
                    run_worker_batch,
                    w_idx + 1,
                    batch,
                    False,
                    fresh_worlds,
                    args.keep_worlds,
                )
                for w_idx, batch in enumerate(batches)
                if batch
            ]
            runners = [f.result() for f in futures]

    total_duration = time.time() - total_start
    passed = sum(r.passed for r in runners)
    failed = sum(r.failed for r in runners)

    print("\n=======================================================")
    print(
        f" Total: {passed + failed} | Passed: \033[92m{passed}\033[0m | Failed: \033[91m{failed}\033[0m | Time: {total_duration:.2f}s"
    )
    print("=======================================================")

    if failed > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()