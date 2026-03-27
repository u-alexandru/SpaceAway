use sa_universe::{generate_nebulae, MasterSeed};

/// Dome radius constant used by the renderer.
const DOME_RADIUS: f64 = 80_000.0;
/// Distance cull threshold in light-years.
const DIST_CULL_LY: f64 = 30_000.0;
/// Minimum angular radius in radians before a nebula is skipped by the GPU.
const MIN_ANGULAR_RADIUS_RAD: f64 = 0.008;
/// Angular size cull threshold in degrees (0.5°).
const ANGULAR_CULL_DEG: f64 = 0.5;

fn angular_size_deg(radius: f64, distance: f64) -> f64 {
    (radius / distance).atan().to_degrees()
}

fn angular_radius_rad(radius: f64, distance: f64) -> f64 {
    (radius / distance).atan()
}

fn distance(ox: f64, oy: f64, oz: f64, nx: f64, ny: f64, nz: f64) -> f64 {
    let dx = nx - ox;
    let dy = ny - oy;
    let dz = nz - oz;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn dome_center(ox: f64, oy: f64, oz: f64, nx: f64, ny: f64, nz: f64, dist: f64) -> [f64; 3] {
    let dx = nx - ox;
    let dy = ny - oy;
    let dz = nz - oz;
    let nx = dx / dist;
    let ny = dy / dist;
    let nz = dz / dist;
    [nx * DOME_RADIUS, ny * DOME_RADIUS, nz * DOME_RADIUS]
}

fn projected_radius(radius: f64, distance: f64) -> f64 {
    angular_radius_rad(radius, distance) * DOME_RADIUS
}

#[test]
fn nebula_diagnostic() {
    let nebulae = generate_nebulae(MasterSeed(42));

    // -------------------------------------------------------------------------
    // Section 1: All 80 nebula properties
    // -------------------------------------------------------------------------
    println!("\n================================================================================");
    println!("  ALL 80 NEBULAE  (seed=42)");
    println!("================================================================================");
    println!(
        "{:>3}  {:>10}  {:>10}  {:>10}  {:>8}  {:>22}  {:>6}  {:>18}",
        "#", "x (ly)", "y (ly)", "z (ly)", "r (ly)", "color [R, G, B]", "opacity", "seed"
    );
    println!("{}", "-".repeat(100));

    for (i, n) in nebulae.iter().enumerate() {
        println!(
            "{:>3}  {:>10.1}  {:>10.1}  {:>10.1}  {:>8.1}  [{:.2}, {:.2}, {:.2}]  {:>7.3}  {:>18}",
            i,
            n.x, n.y, n.z, n.radius,
            n.color[0], n.color[1], n.color[2],
            n.opacity,
            n.seed,
        );
    }

    // -------------------------------------------------------------------------
    // Section 2: Five teleport viewpoints
    // -------------------------------------------------------------------------
    struct Viewpoint {
        label: &'static str,
        x: f64,
        y: f64,
        z: f64,
    }

    // The "near first nebula" viewpoint is computed dynamically below.
    let static_viewpoints = [
        Viewpoint { label: "mid-disc  (27000,0,0)",  x: 27_000.0, y: 0.0, z: 0.0 },
        Viewpoint { label: "above     (0,30000,0)",  x:      0.0, y: 30_000.0, z: 0.0 },
        Viewpoint { label: "edge      (60000,0,0)",  x: 60_000.0, y: 0.0, z: 0.0 },
        Viewpoint { label: "center    (3000,0,0)",   x:  3_000.0, y: 0.0, z: 0.0 },
    ];

    for vp in &static_viewpoints {
        print_viewpoint_analysis(vp.label, vp.x, vp.y, vp.z, &nebulae);
    }

    // -------------------------------------------------------------------------
    // Section 3: Near-nebula viewpoint — 0.5 * radius from first nebula
    // -------------------------------------------------------------------------
    let n0 = &nebulae[0];
    let near_x = n0.x + n0.radius * 0.5;
    let near_y = n0.y;
    let near_z = n0.z;

    let near_label = "near nebula[0] (0.5r offset)";
    print_viewpoint_analysis(near_label, near_x, near_y, near_z, &nebulae);

    // -------------------------------------------------------------------------
    // Section 4: Detailed breakdown for near-nebula case
    // -------------------------------------------------------------------------
    println!("\n================================================================================");
    println!("  NEAR-NEBULA DETAIL  (nebula[0])");
    println!("================================================================================");

    let dist = distance(near_x, near_y, near_z, n0.x, n0.y, n0.z);
    let ang_deg = angular_size_deg(n0.radius, dist);
    let ang_rad = angular_radius_rad(n0.radius, dist);
    let center = dome_center(near_x, near_y, near_z, n0.x, n0.y, n0.z, dist);
    let proj_r = projected_radius(n0.radius, dist);
    let passes_min_threshold = ang_rad >= MIN_ANGULAR_RADIUS_RAD;

    println!("  Observer position     : ({:.2}, {:.2}, {:.2}) ly", near_x, near_y, near_z);
    println!("  Nebula  position      : ({:.2}, {:.2}, {:.2}) ly", n0.x, n0.y, n0.z);
    println!("  Nebula  radius        : {:.2} ly", n0.radius);
    println!("  Distance to nebula    : {:.4} ly", dist);
    println!("  Angular size (deg)    : {:.6}°", ang_deg);
    println!("  Angular radius (rad)  : {:.6} rad", ang_rad);
    println!("  Min threshold (rad)   : {:.3} rad  (0.008)", MIN_ANGULAR_RADIUS_RAD);
    println!("  Passes threshold?     : {}", if passes_min_threshold { "YES" } else { "NO  <-- CULLED" });
    println!();
    println!("  Dome center (GPU)     : [{:.2}, {:.2}, {:.2}]", center[0], center[1], center[2]);
    println!("  Projected radius (GPU): {:.4}  (= ang_rad * {:.0})", proj_r, DOME_RADIUS);
    println!("  Nebula color          : [{:.2}, {:.2}, {:.2}]", n0.color[0], n0.color[1], n0.color[2]);
    println!("  Nebula opacity        : {:.3}", n0.opacity);
}

fn print_viewpoint_analysis(label: &str, ox: f64, oy: f64, oz: f64, nebulae: &[sa_universe::Nebula]) {
    println!("\n================================================================================");
    println!("  VIEWPOINT: {}  observer=({:.0},{:.0},{:.0})", label, ox, oy, oz);
    println!("================================================================================");
    println!(
        "{:>3}  {:>10}  {:>10}  {:>8}  {:>10}  {:>12}  {:>12}  {:>30}  {:>12}",
        "#", "dist (ly)", "r (ly)", "ang (°)", "ang_r (rad)",
        "dist_cull", "ang_cull", "dome_center [x,y,z]", "proj_r"
    );
    println!("{}", "-".repeat(130));

    let mut survive_dist = 0usize;
    let mut survive_ang = 0usize;

    for (i, n) in nebulae.iter().enumerate() {
        let dist = distance(ox, oy, oz, n.x, n.y, n.z);
        let ang_deg = angular_size_deg(n.radius, dist);
        let ang_rad = angular_radius_rad(n.radius, dist);
        let passes_dist = dist <= DIST_CULL_LY;
        let passes_ang = ang_deg >= ANGULAR_CULL_DEG;

        if passes_dist { survive_dist += 1; }
        if passes_ang  { survive_ang  += 1; }

        let center = dome_center(ox, oy, oz, n.x, n.y, n.z, dist);
        let proj_r = projected_radius(n.radius, dist);

        let dist_mark = if passes_dist { "PASS" } else { "CULL" };
        let ang_mark  = if passes_ang  { "PASS" } else { "CULL" };

        println!(
            "{:>3}  {:>10.1}  {:>10.1}  {:>8.4}  {:>11.6}  {:>12}  {:>12}  [{:>8.1},{:>8.1},{:>8.1}]  {:>12.4}",
            i, dist, n.radius, ang_deg, ang_rad,
            dist_mark, ang_mark,
            center[0], center[1], center[2],
            proj_r,
        );
    }

    println!();
    println!(
        "  Survive distance cull (<= {:.0} ly)  : {} / {}",
        DIST_CULL_LY, survive_dist, nebulae.len()
    );
    println!(
        "  Have angular size > {:.1}°            : {} / {}",
        ANGULAR_CULL_DEG, survive_ang, nebulae.len()
    );
    println!(
        "  Survive both culls                    : {} / {}",
        nebulae.iter().filter(|n| {
            let d = distance(ox, oy, oz, n.x, n.y, n.z);
            d <= DIST_CULL_LY && angular_size_deg(n.radius, d) >= ANGULAR_CULL_DEG
        }).count(),
        nebulae.len()
    );
}
