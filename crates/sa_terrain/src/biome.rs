//! Biome color determination by altitude, latitude, and planet sub-type.
//!
//! Matches the color scheme used by sa_render::planet_mesh for visual
//! consistency between the icosphere (from space) and CDLOD terrain (close up).

use sa_universe::PlanetSubType;

/// Determine vertex color from terrain height, latitude, and planet type.
///
/// `height_norm`: height in [0, 1] (0 = lowest terrain, 1 = highest).
/// `latitude`: absolute latitude in [0, 1] (0 = equator, 1 = pole).
pub fn biome_color(sub_type: PlanetSubType, height_norm: f32, latitude: f32) -> [f32; 3] {
    match sub_type {
        PlanetSubType::Barren => {
            let g = 0.3 + height_norm * 0.3;
            [g, g * 0.95, g * 0.9]
        }
        PlanetSubType::Desert => {
            let base = 0.5 + height_norm * 0.3;
            if height_norm > 0.8 {
                [0.6, 0.55, 0.5]
            } else {
                [base, base * 0.85, base * 0.5]
            }
        }
        PlanetSubType::Temperate => {
            if latitude > 0.8 {
                [0.9, 0.92, 0.95]
            } else if height_norm > 0.75 {
                [0.85, 0.87, 0.9]
            } else if height_norm > 0.5 {
                [0.45, 0.38, 0.3]
            } else if height_norm < 0.15 {
                [0.2, 0.3, 0.6]
            } else {
                [0.25, 0.45, 0.2]
            }
        }
        PlanetSubType::Ocean => {
            if height_norm > 0.7 {
                [0.35, 0.4, 0.3]
            } else {
                let depth = 0.2 + height_norm * 0.3;
                [depth * 0.4, depth * 0.5, depth * 1.2]
            }
        }
        PlanetSubType::Frozen => {
            let g = 0.75 + height_norm * 0.2;
            [g, g + 0.02, g + 0.05]
        }
        PlanetSubType::Molten => {
            if height_norm < 0.3 {
                [0.8, 0.2, 0.05]
            } else {
                let g = 0.15 + height_norm * 0.15;
                [g, g * 0.8, g * 0.7]
            }
        }
        _ => {
            let g = 0.4 + height_norm * 0.3;
            [g, g, g]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biome_colors_in_valid_range() {
        let types = [
            PlanetSubType::Barren, PlanetSubType::Desert, PlanetSubType::Temperate,
            PlanetSubType::Ocean, PlanetSubType::Frozen, PlanetSubType::Molten,
        ];
        for sub in types {
            for h in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for lat in [0.0, 0.5, 1.0] {
                    let [r, g, b] = biome_color(sub, h, lat);
                    assert!((0.0..=1.5).contains(&r), "{sub:?} h={h} lat={lat}: r={r}");
                    assert!((0.0..=1.5).contains(&g), "{sub:?} h={h} lat={lat}: g={g}");
                    assert!((0.0..=1.5).contains(&b), "{sub:?} h={h} lat={lat}: b={b}");
                }
            }
        }
    }

    #[test]
    fn temperate_poles_are_icy() {
        let [r, g, _b] = biome_color(PlanetSubType::Temperate, 0.5, 0.9);
        assert!(r > 0.8, "polar r={r} should be bright");
        assert!(g > 0.8, "polar g={g} should be bright");
    }

    #[test]
    fn temperate_lowlands_are_green() {
        let [r, g, _b] = biome_color(PlanetSubType::Temperate, 0.3, 0.3);
        assert!(g > r, "green channel should dominate: r={r} g={g}");
    }
}
