//! Spreading objects out on the XY plane so that their footprints do not overlap ("Arrange").

use glam::DVec2;

/// Where to move each footprint so none overlap: the offsets (XY) to add to each box, in input order.
///
/// Boxes are `(min, max)` corners. They are laid out in rows (tallest first), each row about as wide as the
/// layout is deep, with `gap` between neighbours; the layout keeps the centre of the original boxes.
pub fn arrange(boxes: &[(DVec2, DVec2)], gap: f64) -> Vec<DVec2> {
    if boxes.is_empty() {
        return Vec::new();
    }
    let sizes: Vec<DVec2> = boxes.iter().map(|(lo, hi)| (*hi - *lo).max(DVec2::ZERO)).collect();
    let area: f64 = sizes.iter().map(|s| (s.x + gap) * (s.y + gap)).sum();
    let widest = sizes.iter().map(|s| s.x).fold(0.0, f64::max);
    let row_width = area.sqrt().max(widest);

    // tallest first, so rows are filled with boxes of similar depth
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| sizes[b].y.total_cmp(&sizes[a].y).then(a.cmp(&b)));

    let mut placed = vec![DVec2::ZERO; boxes.len()]; // new lower-left corner of each box
    let (mut x, mut y, mut row_depth) = (0.0, 0.0, 0.0f64);
    for &i in &order {
        if x > 0.0 && x + sizes[i].x > row_width {
            x = 0.0;
            y += row_depth + gap;
            row_depth = 0.0;
        }
        placed[i] = DVec2::new(x, y);
        x += sizes[i].x + gap;
        row_depth = row_depth.max(sizes[i].y);
    }

    // centre the layout where the boxes were
    let layout_max = (0..boxes.len()).map(|i| placed[i] + sizes[i]).fold(DVec2::ZERO, DVec2::max);
    let old_min = boxes.iter().map(|b| b.0).fold(DVec2::splat(f64::INFINITY), DVec2::min);
    let old_max = boxes.iter().map(|b| b.1).fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
    let shift = (old_min + old_max) / 2.0 - layout_max / 2.0;
    (0..boxes.len()).map(|i| placed[i] + shift - boxes[i].0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: (DVec2, DVec2), b: (DVec2, DVec2)) -> bool {
        a.0.x < b.1.x && b.0.x < a.1.x && a.0.y < b.1.y && b.0.y < a.1.y
    }

    #[test]
    fn stacked_boxes_end_up_apart_and_centred() {
        // five boxes of different sizes all on top of each other
        let boxes: Vec<(DVec2, DVec2)> = [(10.0, 10.0), (20.0, 5.0), (8.0, 30.0), (12.0, 12.0), (3.0, 3.0)]
            .iter()
            .map(|&(w, h)| (DVec2::new(-w / 2.0, -h / 2.0), DVec2::new(w / 2.0, h / 2.0)))
            .collect();
        let offsets = arrange(&boxes, 2.0);
        let moved: Vec<(DVec2, DVec2)> = boxes.iter().zip(&offsets).map(|(b, o)| (b.0 + *o, b.1 + *o)).collect();
        for i in 0..moved.len() {
            for j in i + 1..moved.len() {
                assert!(!overlap(moved[i], moved[j]), "boxes {i} and {j} overlap");
            }
        }
        let lo = moved.iter().map(|b| b.0).fold(DVec2::splat(f64::INFINITY), DVec2::min);
        let hi = moved.iter().map(|b| b.1).fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
        assert!(((lo + hi) / 2.0).length() < 1e-9, "layout is centred where the boxes were");
    }

    #[test]
    fn a_single_box_stays_put() {
        let offsets = arrange(&[(DVec2::new(5.0, 5.0), DVec2::new(15.0, 9.0))], 2.0);
        assert!(offsets[0].length() < 1e-9);
    }
}
