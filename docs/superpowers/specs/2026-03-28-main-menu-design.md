# Main Menu Design

## Overview

The main menu is the player's first impression. Instead of a static background, it shows a randomly selected celestial scene from the procedural universe — a different breathtaking view every launch.

## Background Scene

Each time the main menu loads, it randomly selects ONE of these scene types:

| Scene | What the player sees | How to generate |
|-------|---------------------|-----------------|
| **Planet close-up** | A rocky planet filling half the screen, star visible in the distance. Surface detail visible (terrain facets, biome colors). | Pick random star, generate system, select a rocky planet, position camera at 1.5x radius. |
| **Gas giant with rings** | A banded gas giant with ring system, moons visible as small dots. | Search for a ringed gas giant, position camera at 2x radius, slight angle to show rings. |
| **Galaxy panorama** | The Milky Way band stretching across the screen, dense star field. | Position camera in the galactic disc, face toward the core. Use existing sky shader + star field. |
| **Nebula close-up** | A colorful nebula cloud with stars behind it. | Position camera near a generated nebula, face toward it. |
| **Star close-up** | A massive star with corona glow filling the view, convection cells visible. | Position camera at 3x star radius of a bright star. |
| **Binary view** | Two stars of different colors close together, with planets in orbit. | Find two nearby stars in the same sector (rare but striking). |
| **Deep space** | Nearly empty — just a few distant stars and the faint galaxy band. The void. | Position camera far from the galactic plane. Minimal stars. Emphasizes loneliness. |

### Selection weights

| Scene | Weight | Notes |
|-------|--------|-------|
| Planet close-up | 25% | Most common — shows off terrain |
| Gas giant + rings | 20% | Iconic |
| Galaxy panorama | 20% | Shows scale |
| Nebula | 10% | Colorful variety |
| Star close-up | 10% | Dramatic |
| Deep space | 10% | On-theme (loneliness) |
| Binary | 5% | Rare treat |

### Camera behavior

The camera slowly drifts/rotates during the menu (0.5°/second). This creates a living, breathing background without player input. The scene is rendered using the existing renderer — no special menu-only rendering.

### Seed

Use a time-based seed (e.g., system clock seconds) so the scene is different every launch but reproducible within the same second (for screenshots).

## Menu Layout

Minimal, clean, matching the game's cold/stark aesthetic:

```
                    S P A C E A W A Y


                    ▸ Continue
                    ▸ New Game
                    ▸ Settings
                    ▸ Quit


            "The universe is vast and indifferent."
```

- Title: large, spaced-out letters, silver-white
- Menu items: simple text, highlight on hover
- Subtitle: small, dim, rotates through atmospheric quotes
- No fancy UI elements — text only, letting the background scene speak
- Rendered via egui overlay on top of the 3D scene

## Technical Implementation

### Phase 1 (minimum viable)
- On app start, before entering the game loop, generate a random scene
- Render it as background using the existing renderer
- Overlay egui menu on top
- "Continue" loads directly into the ship (current behavior)
- "New Game" resets state (future)

### Phase 2 (polish)
- Slow camera drift animation
- Atmospheric quotes that fade in/out
- Smooth transition from menu to game (camera flies into the ship)
- Music track plays during menu

## Quote Pool

Rotating atmospheric subtitles (one shown per menu visit):

- "The universe is vast and indifferent."
- "Every star is a sun. Every sun, a world."
- "In space, no one can hear you wonder."
- "The cold between stars is patient."
- "You are small. The universe does not care."
- "Mystery is the only compass."
- "The silence out here has weight."
- "Every light in the sky is a place you could go."
- "Fear and wonder are the same thing, out here."
- "The void doesn't end. Neither does curiosity."
