use sa_math::WorldPos;
use sa_universe::*;
use sa_universe::sector::generate_sector;
use sa_universe::star::Star;

#[test]
fn brightness_pipeline_analysis() {
    let uni = Universe::new(MasterSeed(42));
    let stars = uni.visible_stars(WorldPos::ORIGIN, 5);

    println!("\n=== STAR BRIGHTNESS PIPELINE ANALYSIS ===");
    println!("Total visible stars: {}", stars.len());

    // Brightness distribution
    let mut brightnesses: Vec<f32> = stars.iter().map(|s| s.brightness).collect();
    brightnesses.sort_by(|a, b| a.partial_cmp(b).unwrap());

    println!("\n--- Final Brightness Distribution ---");
    println!("Min:    {:.4}", brightnesses.first().unwrap_or(&0.0));
    println!("P10:    {:.4}", brightnesses[brightnesses.len() / 10]);
    println!("P25:    {:.4}", brightnesses[brightnesses.len() / 4]);
    println!("Median: {:.4}", brightnesses[brightnesses.len() / 2]);
    println!("P75:    {:.4}", brightnesses[3 * brightnesses.len() / 4]);
    println!("P90:    {:.4}", brightnesses[9 * brightnesses.len() / 10]);
    println!("Max:    {:.4}", brightnesses.last().unwrap_or(&0.0));

    // Histogram
    println!("\n--- Brightness Histogram ---");
    let buckets = [0.0f32, 0.15, 0.20, 0.25, 0.30, 0.40, 0.50, 0.70, 1.01];
    for i in 0..buckets.len() - 1 {
        let count = brightnesses
            .iter()
            .filter(|&&b| b >= buckets[i] && b < buckets[i + 1])
            .count();
        let pct = count as f64 / brightnesses.len() as f64 * 100.0;
        let bar: String = std::iter::repeat('#').take((pct * 0.5) as usize).collect();
        println!(
            "[{:.2}-{:.2}): {:>5} ({:>5.1}%) {}",
            buckets[i],
            buckets[i + 1],
            count,
            pct,
            bar
        );
    }

    // Get underlying star data
    let sectors = uni.nearby_sectors(WorldPos::ORIGIN, 5);
    let mut all_stars: Vec<(Star, f32)> = Vec::new();
    for coord in &sectors {
        let sector = generate_sector(uni.seed, *coord);
        for ps in &sector.stars {
            let dx = ps.position.x as f32;
            let dy = ps.position.y as f32;
            let dz = ps.position.z as f32;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            all_stars.push((ps.star.clone(), dist));
        }
    }

    // Mass/luminosity distribution
    println!("\n--- Mass Distribution ---");
    let mut masses: Vec<f32> = all_stars.iter().map(|(s, _)| s.mass).collect();
    masses.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("Min:    {:.4}", masses.first().unwrap_or(&0.0));
    println!("P25:    {:.4}", masses[masses.len() / 4]);
    println!("Median: {:.4}", masses[masses.len() / 2]);
    println!("P75:    {:.4}", masses[3 * masses.len() / 4]);
    println!("P90:    {:.4}", masses[9 * masses.len() / 10]);
    println!("Max:    {:.4}", masses.last().unwrap_or(&0.0));

    println!("\n--- Luminosity Distribution ---");
    let mut lums: Vec<f32> = all_stars.iter().map(|(s, _)| s.luminosity).collect();
    lums.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("Min:    {:.6}", lums.first().unwrap_or(&0.0));
    println!("P25:    {:.6}", lums[lums.len() / 4]);
    println!("Median: {:.6}", lums[lums.len() / 2]);
    println!("P75:    {:.6}", lums[3 * lums.len() / 4]);
    println!("P90:    {:.6}", lums[9 * lums.len() / 10]);
    println!("Max:    {:.2}", lums.last().unwrap_or(&0.0));

    // Spectral class distribution
    println!("\n--- Spectral Class Distribution ---");
    for cls in ["O", "B", "A", "F", "G", "K", "M"] {
        let count = all_stars
            .iter()
            .filter(|(s, _)| format!("{:?}", s.spectral_class) == cls)
            .count();
        let pct = count as f64 / all_stars.len() as f64 * 100.0;
        println!("{}: {:>5} ({:>5.1}%)", cls, count, pct);
    }

    // Trace the brightness formula at various distances
    println!("\n--- Brightness Formula: luminosity -> apparent -> final ---");
    println!("{:<10} {:>10} {:>10} {:>10}", "Lumin", "Dist(ly)", "Apparent", "Final_B");
    for dist in [5.0f32, 15.0, 30.0, 50.0] {
        let dist_sq = dist * dist;
        for lum in [0.005, 0.05, 0.5, 1.0, 10.0, 100.0] {
            let apparent = lum / (1.0 + dist_sq * 0.005);
            let brightness = (apparent.sqrt() * 0.4 + 0.15).clamp(0.15, 1.0);
            println!(
                "{:<10.4} {:>10.1} {:>10.6} {:>10.4}",
                lum, dist, apparent, brightness
            );
        }
        println!();
    }

    // Key insight: what fraction of stars have brightness < 0.2?
    let dim_count = brightnesses.iter().filter(|&&b| b < 0.20).count();
    let medium_count = brightnesses
        .iter()
        .filter(|&&b| b >= 0.20 && b < 0.40)
        .count();
    let bright_count = brightnesses.iter().filter(|&&b| b >= 0.40).count();
    println!("--- Summary ---");
    println!(
        "Dim   (<0.20): {:>5} ({:.1}%)",
        dim_count,
        dim_count as f64 / brightnesses.len() as f64 * 100.0
    );
    println!(
        "Medium(0.2-0.4): {:>5} ({:.1}%)",
        medium_count,
        medium_count as f64 / brightnesses.len() as f64 * 100.0
    );
    println!(
        "Bright(>=0.4): {:>5} ({:.1}%)",
        bright_count,
        bright_count as f64 / brightnesses.len() as f64 * 100.0
    );
}
