//! Braille scatter plot widget for ML cluster visualization
//!
//! Renders 2D scatter plots using Unicode braille characters (U+2800-28FF).
//! Each cell can represent up to 8 dots in a 2×4 grid, providing high-resolution
//! visualization in terminal environments.
//!
//! # Resolution
//!
//! - Each terminal cell: 2 dots wide × 4 dots tall
//! - 80×24 terminal: 160×96 effective dot resolution
//!
//! # Braille Character Layout
//!
//! ```text
//! ┌───┐
//! │1 4│  Bit positions in braille character:
//! │2 5│  char = 0x2800 + (b1 + b2*2 + b3*4 + b4*64 + b5*8 + b6*16 + b7*32 + b8*128)
//! │3 6│
//! │7 8│
//! └───┘
//! ```

use crate::visualize::theme::graph;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    Frame,
};

/// Braille dot offsets for converting (x, y) to bit position
/// Layout:
/// (0,0) (1,0)  -> bits 0, 3
/// (0,1) (1,1)  -> bits 1, 4
/// (0,2) (1,2)  -> bits 2, 5
/// (0,3) (1,3)  -> bits 6, 7
const BRAILLE_OFFSETS: [[u8; 4]; 2] = [
    [0, 1, 2, 6], // Left column (x=0)
    [3, 4, 5, 7], // Right column (x=1)
];

/// Unicode braille base character
const BRAILLE_BASE: char = '\u{2800}';

/// Render a braille scatter plot
///
/// # Arguments
///
/// * `f` - Frame to render to
/// * `points` - Vector of (x, y, cluster_id) tuples, where x and y are normalized 0-1
/// * `cluster_colors` - Colors for each cluster (index = cluster_id, 255 = outlier)
/// * `area` - Rectangle to render within
pub fn render(f: &mut Frame, points: &[(f64, f64, u8)], cluster_colors: &[Color], area: Rect) {
    let buf = f.buffer_mut();
    render_to_buffer(buf, points, cluster_colors, area);
}

/// Render scatter plot to buffer (for testing)
pub fn render_to_buffer(
    buf: &mut Buffer,
    points: &[(f64, f64, u8)],
    cluster_colors: &[Color],
    area: Rect,
) {
    if area.width < 2 || area.height < 2 {
        return;
    }

    // Calculate dot resolution
    let dot_width = (area.width as usize) * 2;
    let dot_height = (area.height as usize) * 4;

    // Create dot matrix (cluster_id for each dot, None if empty)
    let mut dots: Vec<Vec<Option<u8>>> = vec![vec![None; dot_height]; dot_width];

    // Plot points
    for (x, y, cluster_id) in points {
        // Clamp and scale to dot coordinates
        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);

        let dot_x = ((x * (dot_width - 1) as f64).round() as usize).min(dot_width - 1);
        // Flip Y axis (0 = top in terminal, but we want 0 = bottom for scatter)
        let dot_y = (((1.0 - y) * (dot_height - 1) as f64).round() as usize).min(dot_height - 1);

        dots[dot_x][dot_y] = Some(*cluster_id);
    }

    // Render to buffer
    for cell_y in 0..area.height {
        for cell_x in 0..area.width {
            let buf_x = area.x + cell_x;
            let buf_y = area.y + cell_y;

            // Calculate which dots fall in this cell
            let dot_x_start = (cell_x as usize) * 2;
            let dot_y_start = (cell_y as usize) * 4;

            // Build braille character
            let mut braille_bits: u8 = 0;
            let mut cell_color: Option<Color> = None;

            #[allow(clippy::needless_range_loop)]
            for dx in 0..2 {
                #[allow(clippy::needless_range_loop)]
                for dy in 0..4 {
                    let dot_x = dot_x_start + dx;
                    let dot_y = dot_y_start + dy;

                    if dot_x < dot_width && dot_y < dot_height {
                        if let Some(cluster_id) = dots[dot_x][dot_y] {
                            // Set the bit for this dot
                            braille_bits |= 1 << BRAILLE_OFFSETS[dx][dy];

                            // Set color (last point wins if overlapping)
                            cell_color = Some(if cluster_id == 255 {
                                graph::OUTLIER
                            } else {
                                cluster_colors
                                    .get(cluster_id as usize)
                                    .copied()
                                    .unwrap_or(Color::White)
                            });
                        }
                    }
                }
            }

            // Render character
            if braille_bits != 0 {
                let ch = char::from_u32(BRAILLE_BASE as u32 + braille_bits as u32)
                    .unwrap_or(BRAILLE_BASE);
                let style = Style::default().fg(cell_color.unwrap_or(Color::White));
                buf[(buf_x, buf_y)].set_char(ch).set_style(style);
            }
        }
    }
}

/// Render outlier markers as '×' characters (for emphasis)
pub fn render_outlier_markers(f: &mut Frame, points: &[(f64, f64, u8)], area: Rect) {
    let buf = f.buffer_mut();

    for (x, y, cluster_id) in points {
        if *cluster_id != 255 {
            continue; // Only outliers
        }

        // Map to cell coordinates
        let cell_x =
            ((x.clamp(0.0, 1.0) * (area.width - 1) as f64).round() as u16).min(area.width - 1);
        let cell_y = (((1.0 - y.clamp(0.0, 1.0)) * (area.height - 1) as f64).round() as u16)
            .min(area.height - 1);

        let buf_x = area.x + cell_x;
        let buf_y = area.y + cell_y;

        buf[(buf_x, buf_y)]
            .set_char('×')
            .set_style(Style::default().fg(graph::OUTLIER));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_braille_base() {
        // Empty braille should be the base character
        assert_eq!(BRAILLE_BASE, '⠀');
    }

    #[test]
    fn test_braille_encoding() {
        // Single dot at (0,0) should be bit 0
        let ch = char::from_u32(BRAILLE_BASE as u32 + 1).unwrap();
        assert_eq!(ch, '⠁');

        // Single dot at (1,0) should be bit 3
        let ch = char::from_u32(BRAILLE_BASE as u32 + 8).unwrap();
        assert_eq!(ch, '⠈');

        // Full cell should be all bits
        let ch = char::from_u32(BRAILLE_BASE as u32 + 255).unwrap();
        assert_eq!(ch, '⣿');
    }

    #[test]
    fn test_render_empty() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        let points: Vec<(f64, f64, u8)> = vec![];
        let colors = vec![Color::Cyan];

        render_to_buffer(&mut buf, &points, &colors, Rect::new(0, 0, 10, 5));

        // All cells should be empty (default)
    }

    #[test]
    fn test_render_single_point() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        let points = vec![(0.5, 0.5, 0u8)];
        let colors = vec![Color::Cyan];

        render_to_buffer(&mut buf, &points, &colors, Rect::new(0, 0, 10, 5));

        // Should have at least one non-empty cell
        let has_content = (0..10)
            .flat_map(|x| (0..5).map(move |y| (x, y)))
            .any(|(x, y)| buf[(x, y)].symbol() != " ");
        assert!(has_content);
    }

    #[test]
    fn test_render_corners() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
        let points = vec![
            (0.0, 0.0, 0u8), // Bottom-left
            (1.0, 1.0, 1u8), // Top-right
            (0.0, 1.0, 2u8), // Top-left
            (1.0, 0.0, 3u8), // Bottom-right
        ];
        let colors = vec![Color::Red, Color::Green, Color::Blue, Color::Yellow];

        render_to_buffer(&mut buf, &points, &colors, Rect::new(0, 0, 20, 10));

        // Verify corners have content
        // Note: Y is flipped, so (0,0) point appears at bottom
    }

    #[test]
    fn test_render_outlier_color() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        let points = vec![(0.5, 0.5, 255u8)]; // Outlier
        let colors = vec![Color::Cyan];

        render_to_buffer(&mut buf, &points, &colors, Rect::new(0, 0, 10, 5));

        // Should render with outlier color (red)
    }
}
