use sa_math::WorldPos;
use sa_universe::*;
use std::time::Instant;

#[test]
fn performance_analysis() {
    println!("\n=== PERFORMANCE ANALYSIS ===\n");

    // 1. Sector generation timing at various radii
    println!("--- Sector Generation Timing ---");
    for radius in [1, 2, 3, 4, 5] {
        let uni = Universe::new(MasterSeed(42));
        let sectors = uni.nearby_sectors(WorldPos::ORIGIN, radius);
        let sector_count = sectors.len();

        let start = Instant::now();
        let stars = uni.visible_stars(WorldPos::ORIGIN, radius);
        let elapsed = start.elapsed();

        println!(
            "Radius {}: {} sectors, {} stars, generated in {:.1}ms",
            radius,
            sector_count,
            stars.len(),
            elapsed.as_secs_f64() * 1000.0,
        );
    }

    // 2. Single sector generation timing
    println!("\n--- Single Sector Timing (100 iterations) ---");
    let start = Instant::now();
    for i in 0..100 {
        let _ = sector::generate_sector(MasterSeed(42), SectorCoord::new(i, 0, 0));
    }
    let elapsed = start.elapsed();
    println!(
        "Avg per sector: {:.2}ms",
        elapsed.as_secs_f64() * 1000.0 / 100.0,
    );

    // 3. Star vertex conversion timing
    let uni = Universe::new(MasterSeed(42));
    let stars = uni.visible_stars(WorldPos::ORIGIN, 5);
    println!("\n--- Vertex Buffer Size ---");
    println!("Stars: {}", stars.len());
    println!("Vertices (6 per star): {}", stars.len() * 6);
    // StarVertex is 32 bytes (3xf32 pos + f32 brightness + 3xf32 color + f32 pad)
    let star_vertex_size = 32;
    println!(
        "Vertex buffer size: {:.1} MB (StarVertex = {} bytes)",
        stars.len() as f64 * star_vertex_size as f64 / 1024.0 / 1024.0,
        star_vertex_size,
    );

    // 4. What if we cull by brightness?
    println!("\n--- Stars by brightness threshold ---");
    for threshold in [0.30, 0.32, 0.35, 0.40, 0.50] {
        let count = stars.iter().filter(|s| s.brightness >= threshold).count();
        println!(
            "brightness >= {:.2}: {:>6} stars ({:>6} vertices, {:.1} MB)",
            threshold,
            count,
            count * 6,
            count as f64 * 32.0 / 1024.0 / 1024.0,
        );
    }

    // 5. What if we reduce query radius?
    println!("\n--- Star count vs query radius ---");
    for radius in [2, 3, 4, 5] {
        let uni = Universe::new(MasterSeed(42));
        let stars = uni.visible_stars(WorldPos::ORIGIN, radius);
        let bright = stars.iter().filter(|s| s.brightness >= 0.32).count();
        println!(
            "Radius {}: {:>6} total, {:>6} bright (>=0.32)",
            radius,
            stars.len(),
            bright,
        );
    }
}
