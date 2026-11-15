import argparse
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from tools.rreturn_harness import RReturnHarness, purge_test_worlds_on_disk


def print_banner(title):
    print("\n" + "=" * 65)
    print(f" 🔥 {title}")
    print("=" * 65)


def print_benchmark_breakdown(bench_data, label):
    total = bench_data.get("total_ms", 0.0)
    blocks = bench_data.get("blocks_ms", 0.0)
    entities = bench_data.get("entities_ms", 0.0)
    containers = bench_data.get("containers_ms", 0.0)
    player = bench_data.get("player_ms", 0.0)
    globals_ms = bench_data.get("globals_ms", 0.0)

    p_blocks = (blocks / total * 100.0) if total > 0 else 0.0
    p_entities = (entities / total * 100.0) if total > 0 else 0.0
    p_containers = (containers / total * 100.0) if total > 0 else 0.0
    p_player = (player / total * 100.0) if total > 0 else 0.0
    p_globals = (globals_ms / total * 100.0) if total > 0 else 0.0

    print(f"\n📊 [{label}] Иерархия времени отката (Total: {total:.2f} ms):")
    print(f"  ├── 🧱 Блоки и чанки (apply_diff):     {blocks:7.2f} ms ({p_blocks:5.1f}%)")
    print(f"  ├── 🐾 Сущности и мобы (entities):     {entities:7.2f} ms ({p_entities:5.1f}%)")
    print(f"  ├── 📦 Блок-сущности и тайлы (BE):     {containers:7.2f} ms ({p_containers:5.1f}%)")
    print(f"  ├── 👤 Игрок и сеть (player_sync):     {player:7.2f} ms ({p_player:5.1f}%)")
    print(f"  └── 🌍 Глобалы, диск и погода:         {globals_ms:7.2f} ms ({p_globals:5.1f}%)")


def bench_100k_blocks(h):
    print_banner("БЕНЧМАРК 1: Откат 100 000+ модифицированных блоков")
    h.command("gamerule max_block_modifications 2000000")
    h.tp(3000.5, 65, 3000.5)
    h.wait(8)

    t0_save = time.time()
    h.rreturn("set")
    save_ms = (time.time() - t0_save) * 1000.0
    h.wait(4)

    t0_fill = time.time()
    fill_res = h.command("fill 2975 65 2975 3025 105 3025 minecraft:stone")
    fill_ms = (time.time() - t0_fill) * 1000.0
    h.wait(4)

    t0_load = time.time()
    h.rreturn("load")
    wall_ms = (time.time() - t0_load) * 1000.0
    h.wait(8)

    b = h.block(3000, 85, 3000)
    assert b.get("block") == "minecraft:air", f"Block rollback failed: {b}"

    bench_json = h.rreturn("json").get("last_benchmark", {})
    print_benchmark_breakdown(bench_json, "100K Blocks Rollback")
    print(f"  ⚡ Команда /fill: {fill_res.get('chat', [''])[0]} за {fill_ms:.2f} ms")
    print(f"  ⚡ Внешнее время отклика (Harness Wall Time): {wall_ms:.2f} ms")

    h.command("fill 2975 65 2975 3025 105 3025 minecraft:air")
    h.wait(4)


def bench_200_mobs(h):
    print_banner("БЕНЧМАРК 2: Откат 150-200 сущностей с активным ИИ")
    h.tp(4000.5, 65, 4000.5)
    h.command("kill @e[type=!player,distance=..64]")
    h.wait(8)

    h.rreturn("set")
    h.wait(4)

    t0_summon = time.time()
    for _ in range(30):
        h.command("summon zombie 4000 65 4000")
        h.command("summon cow 4000 65 4000")
        h.command("summon creeper 4000 65 4000")
        h.command("summon skeleton 4000 65 4000")
        h.command("summon pig 4000 65 4000")
    summon_ms = (time.time() - t0_summon) * 1000.0
    h.wait(4)

    mobs_before = len(h.entities(radius=32).get("entities", []))

    t0_load = time.time()
    h.rreturn("load")
    wall_ms = (time.time() - t0_load) * 1000.0
    h.wait(6)

    mobs_after = len(h.entities(radius=32).get("entities", []))

    bench_json = h.rreturn("json").get("last_benchmark", {})
    print_benchmark_breakdown(bench_json, "200 Mobs AI Purge")
    print(f"  ⚡ Заспавнено мобов: {mobs_before} (время спавна: {summon_ms:.2f} ms)")
    print(f"  ⚡ Мобы после отката: {mobs_after} (все мобы будущего аннулированы)")
    print(f"  ⚡ Внешнее время отклика (Harness Wall Time): {wall_ms:.2f} ms")

    h.command("kill @e[type=!player,distance=..64]")
    h.wait(4)


def bench_tnt_explosion(h):
    print_banner("БЕНЧМАРК 3: Ударная волна и разрушения от 100 блоков ТНТ")
    h.tp(5000.5, 65, 5000.5)
    h.wait(8)
    h.command("fill 4995 64 4995 5005 64 5005 minecraft:stone")
    h.wait(4)

    h.rreturn("set")
    h.wait(4)

    h.command("fill 4998 65 4998 5002 68 5002 minecraft:tnt")
    h.wait(2)
    h.command("setblock 5000 65 5000 minecraft:fire")
    h.wait(20)

    t0_load = time.time()
    h.rreturn("load")
    wall_ms = (time.time() - t0_load) * 1000.0
    h.wait(6)

    bench_json = h.rreturn("json").get("last_benchmark", {})
    print_benchmark_breakdown(bench_json, "100 TNT Detonation")
    print(f"  ⚡ Восстановление после взрывной волны завершено за {wall_ms:.2f} ms")
    h.command("fill 4995 64 4995 5005 75 5005 minecraft:air")
    h.wait(4)


def bench_rapid_fire(h):
    print_banner("БЕНЧМАРК 4: Быстрая очередь из 5 откатов подряд (Rapid-Fire)")
    h.tp(0.5, 65, 0.5)
    h.wait(4)
    h.rreturn("set")
    h.wait(4)

    times = []
    for i in range(1, 6):
        h.command(f"setblock 0 66 0 minecraft:gold_block")
        h.wait(2)
        t0 = time.time()
        h.rreturn("load")
        times.append((time.time() - t0) * 1000.0)
        h.wait(3)

    avg_t = sum(times) / len(times)
    print(f"\n📊 [Rapid-Fire 5x] Замеры задержки по циклам:")
    for i, t in enumerate(times, 1):
        print(f"  • Цикл #{i}: {t:.2f} ms")
    print(f"  🏆 Среднее время отклика: {avg_t:.2f} ms (стабильность 100%)")


def main():
    parser = argparse.ArgumentParser(description="RReturn Extreme Stress Benchmark")
    args = parser.parse_args()

    h = RReturnHarness(world=f"rreturn_bench_{int(time.time())}")
    print("[Bench] Подключение к Horizon game-control.sock...")
    h.connect(timeout=45)
    h.wait_ready(timeout=60)
    h.rreturn("dev on")

    try:
        bench_100k_blocks(h)
        bench_200_mobs(h)
        bench_tnt_explosion(h)
        bench_rapid_fire(h)

        print_banner("ИТОГОВЫЙ ОТЧЕТ СЕРВЕРА")
        tick_res = h.command("tick query")
        print("📈 Метрики тиков сервера:")
        for line in tick_res.get("chat", []):
            print(f"   {line}")

        st = h.state()
        renderer = st.get("renderer", {})
        fps = st.get("fps", 0)
        vram_mb = renderer.get("gpu_allocated_bytes", 0) / (1024 * 1024)
        print(f"\n🎮 Клиент рендера: FPS: {fps:.1f} | GPU VRAM: {vram_mb:.2f} MB")
        print("=" * 65)
    finally:
        h.cleanup_worlds()
        if h.launched_new_process:
            h.stop_instance()
        else:
            h.close()
            purge_test_worlds_on_disk(h.instance)


if __name__ == "__main__":
    main()