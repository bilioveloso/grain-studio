use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GuideOrientation {
    Vertical,
    Horizontal,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct GuidePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GuideLine {
    pub id: String,
    #[serde(rename = "type")]
    pub orientation: GuideOrientation,
    pub p1: GuidePoint,
    pub p2: GuidePoint,
}

#[derive(Debug, Clone)]
pub struct GuidedResult {
    pub forward_h: [[f64; 3]; 3],
    pub debug_vp: [Option<[f64; 2]>; 2],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuidedResultJson {
    pub forward_h: [f64; 9],
    pub crop: [f64; 4],
    pub valid: bool,
    pub debug_vp: [Option<[f64; 2]>; 2],
}

const EPS_INF: f64 = 1e-9;
const EPS_DEG: f64 = 1e-9;

fn unit_norm(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n < EPS_INF {
        v
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn matmul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

fn hom_mul(h: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        h[0][0] * v[0] + h[0][1] * v[1] + h[0][2] * v[2],
        h[1][0] * v[0] + h[1][1] * v[1] + h[1][2] * v[2],
        h[2][0] * v[0] + h[2][1] * v[1] + h[2][2] * v[2],
    ]
}

fn project_h(h: &[[f64; 3]; 3], x: f64, y: f64) -> Option<(f64, f64)> {
    let w = h[2][0] * x + h[2][1] * y + h[2][2];
    if w.abs() < 1e-6 {
        return None;
    }
    Some((
        (h[0][0] * x + h[0][1] * y + h[0][2]) / w,
        (h[1][0] * x + h[1][1] * y + h[1][2]) / w,
    ))
}

fn rotation(theta: f64) -> [[f64; 3]; 3] {
    let (s, c) = theta.sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

fn det3(h: &[[f64; 3]; 3]) -> f64 {
    h[0][0] * (h[1][1] * h[2][2] - h[1][2] * h[2][1])
        - h[0][1] * (h[1][0] * h[2][2] - h[1][2] * h[2][0])
        + h[0][2] * (h[1][0] * h[2][1] - h[1][1] * h[2][0])
}

fn uv_to_centered(p: GuidePoint, width: f64, height: f64) -> (f64, f64) {
    (p.x * width - width / 2.0, p.y * height - height / 2.0)
}

fn line_centered(line: &GuideLine, width: f64, height: f64) -> ((f64, f64), (f64, f64)) {
    (
        uv_to_centered(line.p1, width, height),
        uv_to_centered(line.p2, width, height),
    )
}

fn is_line_valid(line: &GuideLine, width: f64, height: f64) -> bool {
    let ((x1, y1), (x2, y2)) = line_centered(line, width, height);
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = dx.hypot(dy);
    let min_len = 0.02 * width.hypot(height);
    if length < min_len {
        return false;
    }
    let tan_35 = 35.0_f64.to_radians().tan();
    match line.orientation {
        GuideOrientation::Vertical => dx.abs() <= dy.abs() * tan_35,
        GuideOrientation::Horizontal => dy.abs() <= dx.abs() * tan_35,
    }
}

fn filter_valid_lines(
    lines: &[GuideLine],
    width: f64,
    height: f64,
) -> (Vec<GuideLine>, Vec<GuideLine>) {
    let mut verts = Vec::new();
    let mut hors = Vec::new();
    for line in lines {
        if !is_line_valid(line, width, height) {
            continue;
        }
        match line.orientation {
            GuideOrientation::Vertical if verts.len() < 2 => verts.push(line.clone()),
            GuideOrientation::Horizontal if hors.len() < 2 => hors.push(line.clone()),
            _ => {}
        }
    }
    (verts, hors)
}

pub fn count_valid_lines(lines: &[GuideLine], width: f64, height: f64) -> usize {
    let (verts, hors) = filter_valid_lines(lines, width, height);
    verts.len() + hors.len()
}

fn vanishing_point(
    l1: &GuideLine,
    l2: &GuideLine,
    width: f64,
    height: f64,
) -> ([f64; 3], Option<[f64; 2]>) {
    let (p1, p2) = line_centered(l1, width, height);
    let (q1, q2) = line_centered(l2, width, height);
    let l1h = unit_norm(cross([p1.0, p1.1, 1.0], [p2.0, p2.1, 1.0]));
    let l2h = unit_norm(cross([q1.0, q1.1, 1.0], [q2.0, q2.1, 1.0]));
    let v = unit_norm(cross(l1h, l2h));
    let q = if v[2].abs() >= EPS_INF {
        Some([v[0] / v[2], v[1] / v[2]])
    } else {
        None
    };
    (v, q)
}

fn primary_vertical(v: [f64; 3], q_opt: Option<[f64; 2]>, height: f64) -> Option<[[f64; 3]; 3]> {
    if let Some(q) = q_opt {
        let d = q[0].hypot(q[1]);
        if d <= 0.2 * height {
            return None;
        }

        let theta = if q[1] < 0.0 {
            q[0].atan2(-q[1])
        } else {
            q[0].atan2(q[1])
        };

        let r = rotation(-theta);
        let q_rot = hom_mul(&r, [q[0], q[1], 1.0]);
        let qy_prime = q_rot[1] / q_rot[2];

        if qy_prime.abs() < 1e-3 {
            return None;
        }

        let p = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0 / qy_prime, 1.0],
        ];
        Some(matmul(&p, &r))
    } else {
        let mut dx = v[0];
        let mut dy = v[1];
        if dy < 0.0 {
            dx = -dx;
            dy = -dy;
        }
        let theta = dx.atan2(dy);
        Some(rotation(-theta))
    }
}

fn primary_horizontal(v: [f64; 3], q_opt: Option<[f64; 2]>, width: f64) -> Option<[[f64; 3]; 3]> {
    if let Some(q) = q_opt {
        let d = q[0].hypot(q[1]);
        if d <= 0.2 * width {
            return None;
        }

        let theta = if q[0] < 0.0 {
            (-q[1]).atan2(-q[0])
        } else {
            q[1].atan2(q[0])
        };

        let r = rotation(-theta);
        let q_rot = hom_mul(&r, [q[0], q[1], 1.0]);
        let qx_prime = q_rot[0] / q_rot[2];

        if qx_prime.abs() < 1e-3 {
            return None;
        }

        let p = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0 / qx_prime, 0.0, 1.0],
        ];
        Some(matmul(&p, &r))
    } else {
        let mut dx = v[0];
        let mut dy = v[1];
        if dx < 0.0 {
            dx = -dx;
            dy = -dy;
        }
        let theta = dy.atan2(dx);
        Some(rotation(-theta))
    }
}

fn secondary_two_horizontal(h_p: &[[f64; 3]; 3], v: [f64; 3], width: f64) -> Option<[[f64; 3]; 3]> {
    let vp = unit_norm(hom_mul(h_p, v));
    if vp[2].abs() >= EPS_INF {
        let qx = vp[0] / vp[2];
        let qy = vp[1] / vp[2];
        let e = qx;
        if e.abs() <= 0.2 * width {
            return None;
        }
        if qx.abs() < EPS_DEG {
            return None;
        }
        let k = -qy / qx;
        let keystone = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0 / e, 0.0, 1.0]];
        let shear = [[1.0, 0.0, 0.0], [k, 1.0, 0.0], [0.0, 0.0, 1.0]];
        Some(matmul(&shear, &matmul(&keystone, h_p)))
    } else {
        if vp[0].abs() < EPS_DEG {
            return None;
        }
        let k = -vp[1] / vp[0];
        let shear = [[1.0, 0.0, 0.0], [k, 1.0, 0.0], [0.0, 0.0, 1.0]];
        Some(matmul(&shear, h_p))
    }
}

fn secondary_one_line(
    h: &[[f64; 3]; 3],
    line: &GuideLine,
    is_vertical: bool,
    width: f64,
    height: f64,
) -> [[f64; 3]; 3] {
    let (p1, p2) = line_centered(line, width, height);
    let Some((x1, y1)) = project_h(h, p1.0, p1.1) else {
        return *h;
    };
    let Some((x2, y2)) = project_h(h, p2.0, p2.1) else {
        return *h;
    };
    let dx = x2 - x1;
    let dy = y2 - y1;
    if is_vertical {
        if dy.abs() < EPS_DEG {
            return *h;
        }
        let k = -dx / dy;
        let shear = [[1.0, k, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        matmul(&shear, h)
    } else {
        if dx.abs() < EPS_DEG {
            return *h;
        }
        let k = -dy / dx;
        let shear = [[1.0, 0.0, 0.0], [k, 1.0, 0.0], [0.0, 0.0, 1.0]];
        matmul(&shear, h)
    }
}

fn solve_1v1h(vert: &GuideLine, hor: &GuideLine, width: f64, height: f64) -> Option<[[f64; 3]; 3]> {
    let (hp1, hp2) = line_centered(hor, width, height);
    let mut dx = hp2.0 - hp1.0;
    let mut dy = hp2.1 - hp1.1;
    if dx < 0.0 {
        dx = -dx;
        dy = -dy;
    }
    let theta = dy.atan2(dx);
    let r = rotation(-theta);
    Some(secondary_one_line(&r, vert, true, width, height))
}

fn quad_ok(pts: &[(f64, f64); 4]) -> bool {
    let mut sign = 0.0;
    for i in 0..4 {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % 4];
        let (x2, y2) = pts[(i + 2) % 4];
        let c = (x1 - x0) * (y2 - y1) - (y1 - y0) * (x2 - x1);
        if c.abs() < 1e-12 {
            continue;
        }
        let s = c.signum();
        if sign == 0.0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    sign > 0.0
}

fn finalize_h(mut h: [[f64; 3]; 3], width: f64, height: f64) -> Option<[[f64; 3]; 3]> {
    let scale = h[2][2];
    if scale.abs() > 1e-15 {
        for row in &mut h {
            for v in row {
                *v /= scale;
            }
        }
    }
    if det3(&h).abs() < 1e-9 {
        return None;
    }
    let hw = width / 2.0;
    let hh = height / 2.0;
    let src = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
    let mut dest = [(0.0, 0.0); 4];
    for (i, (x, y)) in src.iter().enumerate() {
        let w = h[2][0] * x + h[2][1] * y + h[2][2];
        if w.abs() < 1e-6 {
            return None;
        }
        dest[i] = (
            (h[0][0] * x + h[0][1] * y + h[0][2]) / w,
            (h[1][0] * x + h[1][1] * y + h[1][2]) / w,
        );
    }
    if !quad_ok(&dest) {
        return None;
    }
    Some(h)
}

pub fn compute_guided_homography(
    lines: &[GuideLine],
    width: f64,
    height: f64,
) -> Option<GuidedResult> {
    let (verts, hors) = filter_valid_lines(lines, width, height);
    if verts.len() + hors.len() < 2 {
        return None;
    }

    let mut debug_vp = [None, None];
    let v_vp = if verts.len() == 2 {
        Some(vanishing_point(&verts[0], &verts[1], width, height))
    } else {
        None
    };
    let h_vp = if hors.len() == 2 {
        Some(vanishing_point(&hors[0], &hors[1], width, height))
    } else {
        None
    };
    if let Some((_, q)) = v_vp {
        debug_vp[0] = q;
    }
    if let Some((_, q)) = h_vp {
        debug_vp[1] = q;
    }

    let h = if verts.len() == 2 {
        let (v, q) = v_vp.unwrap();
        let mut hp = primary_vertical(v, q, height)?;
        if hors.len() == 2 {
            let (vh, _) = h_vp.unwrap();
            hp = secondary_two_horizontal(&hp, vh, width)?;
        } else if hors.len() == 1 {
            hp = secondary_one_line(&hp, &hors[0], false, width, height);
        }
        hp
    } else if hors.len() == 2 {
        let (v, q) = h_vp.unwrap();
        let mut hp = primary_horizontal(v, q, width)?;
        if verts.len() == 1 {
            hp = secondary_one_line(&hp, &verts[0], true, width, height);
        }
        hp
    } else {
        solve_1v1h(&verts[0], &hors[0], width, height)?
    };

    let forward_h = finalize_h(h, width, height)?;
    Some(GuidedResult {
        forward_h,
        debug_vp,
    })
}

fn clip_poly_edge(
    poly: &[(f64, f64)],
    inside: impl Fn(f64, f64) -> bool,
    intersect: impl Fn((f64, f64), (f64, f64)) -> (f64, f64),
) -> Vec<(f64, f64)> {
    if poly.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = inside(prev.0, prev.1);
    for &curr in poly {
        let curr_in = inside(curr.0, curr.1);
        if curr_in {
            if !prev_in {
                out.push(intersect(prev, curr));
            }
            out.push(curr);
        } else if prev_in {
            out.push(intersect(prev, curr));
        }
        prev = curr;
        prev_in = curr_in;
    }
    out
}

fn sutherland_hodgman_unit_square(poly: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut p = poly.to_vec();
    p = clip_poly_edge(
        &p,
        |x, _| x >= 0.0,
        |a, b| {
            let t = (0.0 - a.0) / (b.0 - a.0);
            (0.0, a.1 + t * (b.1 - a.1))
        },
    );
    p = clip_poly_edge(
        &p,
        |x, _| x <= 1.0,
        |a, b| {
            let t = (1.0 - a.0) / (b.0 - a.0);
            (1.0, a.1 + t * (b.1 - a.1))
        },
    );
    p = clip_poly_edge(
        &p,
        |_, y| y >= 0.0,
        |a, b| {
            let t = (0.0 - a.1) / (b.1 - a.1);
            (a.0 + t * (b.0 - a.0), 0.0)
        },
    );
    p = clip_poly_edge(
        &p,
        |_, y| y <= 1.0,
        |a, b| {
            let t = (1.0 - a.1) / (b.1 - a.1);
            (a.0 + t * (b.0 - a.0), 1.0)
        },
    );
    p
}

fn x_bounds_at(poly: &[(f64, f64)], y: f64) -> Option<(f64, f64)> {
    let mut xs = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % n];
        let dy = y2 - y1;
        if dy.abs() <= 1e-15 {
            if (y1 - y).abs() < 1e-12 {
                xs.push(x1);
                xs.push(x2);
            }
            continue;
        }
        let t = (y - y1) / dy;
        if (-1e-9..=1.0 + 1e-9).contains(&t) {
            xs.push(x1 + t * (x2 - x1));
        }
    }
    if xs.is_empty() {
        return None;
    }
    let x_l = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_r = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if x_r <= x_l { None } else { Some((x_l, x_r)) }
}

fn slab_rect(poly: &[(f64, f64)], y1: f64, y2: f64) -> Option<[f64; 4]> {
    let (l1, r1) = x_bounds_at(poly, y1)?;
    let (l2, r2) = x_bounds_at(poly, y2)?;
    let x1 = l1.max(l2);
    let x2 = r1.min(r2);
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    Some([x1, y1, x2 - x1, y2 - y1])
}

pub fn compute_max_inscribed_crop(forward_h: &[[f64; 3]; 3], width: f64, height: f64) -> [f64; 4] {
    let hw = width / 2.0;
    let hh = height / 2.0;
    let src = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
    let mut quad = Vec::with_capacity(4);
    for (x, y) in src {
        let w = forward_h[2][0] * x + forward_h[2][1] * y + forward_h[2][2];
        if w.abs() < 1e-6 {
            return [0.0, 0.0, 1.0, 1.0];
        }
        let px = (forward_h[0][0] * x + forward_h[0][1] * y + forward_h[0][2]) / w;
        let py = (forward_h[1][0] * x + forward_h[1][1] * y + forward_h[1][2]) / w;
        quad.push(((px + hw) / width, (py + hh) / height));
    }
    let poly = sutherland_hodgman_unit_square(&quad);
    if poly.len() < 3 {
        return [0.0, 0.0, 1.0, 1.0];
    }

    let y_min = poly.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let y_max = poly.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    if y_max - y_min < 1e-12 {
        return [0.0, 0.0, 1.0, 1.0];
    }

    const SAMPLES: usize = 96;
    let mut best = [0.0, 0.0, 1.0, 1.0];
    let mut best_area = 0.0;
    let mut best_i = 0usize;
    let mut best_j = 1usize;
    for i in 0..SAMPLES {
        for j in (i + 1)..SAMPLES {
            let y1 = y_min + (y_max - y_min) * (i as f64) / ((SAMPLES - 1) as f64);
            let y2 = y_min + (y_max - y_min) * (j as f64) / ((SAMPLES - 1) as f64);
            if let Some(rect) = slab_rect(&poly, y1, y2) {
                let area = rect[2] * rect[3];
                if area > best_area {
                    best_area = area;
                    best = rect;
                    best_i = i;
                    best_j = j;
                }
            }
        }
    }

    let step = (y_max - y_min) / ((SAMPLES - 1) as f64);
    let mut y1 = y_min + step * (best_i as f64);
    let mut y2 = y_min + step * (best_j as f64);
    let mut half = step;
    for _ in 0..6 {
        half *= 0.5;
        for (dy1, dy2) in [
            (-half, 0.0),
            (half, 0.0),
            (0.0, -half),
            (0.0, half),
            (-half, -half),
            (-half, half),
            (half, -half),
            (half, half),
        ] {
            let ny1 = (y1 + dy1).clamp(y_min, y_max);
            let ny2 = (y2 + dy2).clamp(y_min, y_max);
            if ny2 <= ny1 {
                continue;
            }
            if let Some(rect) = slab_rect(&poly, ny1, ny2) {
                let area = rect[2] * rect[3];
                if area > best_area {
                    best_area = area;
                    best = rect;
                    y1 = ny1;
                    y2 = ny2;
                }
            }
        }
    }

    best
}

fn flatten_row_major(h: &[[f64; 3]; 3]) -> [f64; 9] {
    [
        h[0][0], h[0][1], h[0][2], h[1][0], h[1][1], h[1][2], h[2][0], h[2][1], h[2][2],
    ]
}

#[tauri::command]
pub fn calculate_guided_perspective(
    lines: Vec<GuideLine>,
    width: f64,
    height: f64,
) -> GuidedResultJson {
    match compute_guided_homography(&lines, width, height) {
        Some(result) => GuidedResultJson {
            forward_h: flatten_row_major(&result.forward_h),
            crop: compute_max_inscribed_crop(&result.forward_h, width, height),
            valid: true,
            debug_vp: result.debug_vp,
        },
        None => GuidedResultJson {
            forward_h: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            crop: [0.0, 0.0, 1.0, 1.0],
            valid: false,
            debug_vp: [None, None],
        },
    }
}

/// Lines leaning further than this from upright are not treated as verticals.
const MAX_TILT_DEGREES: u32 = 20;

/// Auto Upright (Lightroom's Vertical mode): the two strongest near-vertical straight edges, as guide lines for
/// the same solver the guided tool uses. Horizontals are left out on purpose: in streets and interiors most
/// "horizontal" edges recede into the distance, and forcing them level twists the whole photo. `gray` is the photo in the solver's frame (as loaded, before
/// orientation steps and flips); points come back as 0..1 fractions of it.
pub fn detect_guide_lines(gray: &image::GrayImage) -> Vec<GuideLine> {
    use imageproc::hough::{LineDetectionOptions, detect_lines, intersection_points};

    let (w, h) = gray.dimensions();
    if w < 32 || h < 32 {
        return Vec::new();
    }
    // Stretched to the full range first, so dark linear RAW data gives the same edges as a bright JPEG.
    let stretched = stretch_to_full_range(gray);
    let edges = imageproc::edges::canny(&stretched, 40.0, 110.0);
    let candidates = detect_lines(
        &edges,
        LineDetectionOptions {
            vote_threshold: (w.min(h) / 6).max(24),
            suppression_radius: 6,
        },
    );

    // (position across the frame 0..1, edge pixels along it, endpoints)
    let mut scored = Vec::new();
    for line in candidates {
        let angle = line.angle_in_degrees;
        if angle > MAX_TILT_DEGREES && angle < 180 - MAX_TILT_DEGREES {
            continue;
        }
        let Some((a, b)) = intersection_points(line, w, h) else {
            continue;
        };
        let (hits, length) = edge_support(&edges, a, b);
        // A real edge (a wall, a horizon) covers a good share of the line; texture only touches it here and there.
        if (hits as f32) < length * 0.25 {
            continue;
        }
        scored.push(((a.0 + b.0) / 2.0 / w as f32, hits, a, b));
    }
    scored.sort_by(|x, y| y.1.cmp(&x.1));

    let mut picked = Vec::new();
    let mut positions: Vec<f32> = Vec::new();
    for (position, _, a, b) in &scored {
        if positions.len() == 2 {
            break;
        }
        // Two lines far apart give a steady vanishing point; neighbours are the same edge twice.
        if positions.iter().any(|p| (p - position).abs() < 0.15) {
            continue;
        }
        positions.push(*position);
        picked.push(GuideLine {
            id: format!("auto-vertical-{}", positions.len()),
            orientation: GuideOrientation::Vertical,
            p1: GuidePoint {
                x: a.0 as f64 / w as f64,
                y: a.1 as f64 / h as f64,
            },
            p2: GuidePoint {
                x: b.0 as f64 / w as f64,
                y: b.1 as f64 / h as f64,
            },
        });
    }
    picked
}

/// Linear stretch from the 1st to the 99th percentile. (Histogram equalisation wipes out edges when most of
/// the frame is one tone, like a sky.)
fn stretch_to_full_range(gray: &image::GrayImage) -> image::GrayImage {
    let mut histogram = [0u32; 256];
    for p in gray.pixels() {
        histogram[p[0] as usize] += 1;
    }
    let total = gray.width() * gray.height();
    let percentile = |target: u32| {
        let mut seen = 0;
        for (value, count) in histogram.iter().enumerate() {
            seen += count;
            if seen >= target {
                return value as u8;
            }
        }
        255
    };
    let (low, high) = (percentile(total / 100), percentile(total - total / 100));
    if high <= low {
        return gray.clone();
    }
    let scale = 255.0 / (high - low) as f32;
    image::GrayImage::from_fn(gray.width(), gray.height(), |x, y| {
        let v = gray.get_pixel(x, y)[0].clamp(low, high);
        image::Luma([((v - low) as f32 * scale) as u8])
    })
}

/// Edge pixels within a pixel of the segment, and the segment's length.
fn edge_support(edges: &image::GrayImage, a: (f32, f32), b: (f32, f32)) -> (u32, f32) {
    let (w, h) = edges.dimensions();
    let length = (b.0 - a.0).hypot(b.1 - a.1).max(1.0);
    let steps = length as u32;
    let mut hits = 0;
    for i in 0..=steps {
        let t = i as f32 / steps.max(1) as f32;
        let (x, y) = ((a.0 + (b.0 - a.0) * t) as i32, (a.1 + (b.1 - a.1) * t) as i32);
        let near = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
            let (px, py) = (x + dx, y + dy);
            px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h && edges.get_pixel(px as u32, py as u32)[0] > 0
        });
        if near {
            hits += 1;
        }
    }
    (hits, length)
}

#[cfg(test)]
mod upright_tests {
    use super::*;
    use imageproc::drawing::draw_polygon_mut;
    use imageproc::point::Point;

    #[test]
    fn finds_two_leaning_walls_and_ignores_the_horizon() {
        let mut img = image::GrayImage::from_pixel(400, 300, image::Luma([30]));
        let white = image::Luma([230]);
        // Two walls leaning towards each other (a keystoned building) and a slightly tilted horizon.
        draw_polygon_mut(&mut img, &[Point::new(96, 0), Point::new(104, 0), Point::new(114, 299), Point::new(106, 299)], white);
        draw_polygon_mut(&mut img, &[Point::new(296, 0), Point::new(304, 0), Point::new(294, 299), Point::new(286, 299)], white);
        draw_polygon_mut(&mut img, &[Point::new(0, 148), Point::new(399, 156), Point::new(399, 164), Point::new(0, 156)], white);

        let lines = detect_guide_lines(&img);
        let verticals: Vec<f64> = lines
            .iter()
            .filter(|l| l.orientation == GuideOrientation::Vertical)
            .map(|l| (l.p1.x + l.p2.x) / 2.0)
            .collect();
        let horizontals = lines.iter().filter(|l| l.orientation == GuideOrientation::Horizontal).count();
        assert_eq!(verticals.len(), 2, "{lines:?}");
        assert!(verticals.iter().any(|x| (x - 0.26).abs() < 0.05), "{verticals:?}");
        assert!(verticals.iter().any(|x| (x - 0.74).abs() < 0.05), "{verticals:?}");
        assert_eq!(horizontals, 0, "{lines:?}");
        assert!(compute_guided_homography(&lines, 400.0, 300.0).is_some());
    }
}
