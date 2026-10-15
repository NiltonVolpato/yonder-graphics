use yonder_core::{
    meet, orientation, ConicMatrix, Coord3, Coord4, Line3, Matrix3x3, Matrix4x4, Orientation,
};

#[test]
fn test_stolfi_antipodal_consistency() {
    let p = Coord3::point(10, 20);
    let antipodal = p.antipodal();

    // Both project to Euclidean (10, 20)
    assert_eq!(p.to_euclidean(), Some((10, 20)));
    assert_eq!(antipodal.to_euclidean(), Some((10, 20)));

    // But have opposite universe signs
    assert!(p.is_front());
    assert!(antipodal.is_antipodal());

    // Joining with third point flips orientation for antipodal point
    let q = Coord3::point(20, 20);
    let r = Coord3::point(15, 30);

    let orient_front = orientation(p, q, r);
    let orient_rear = orientation(antipodal, q, r);

    assert_eq!(orient_front, Orientation::CounterClockwise);
    assert_eq!(orient_rear, Orientation::Clockwise);
}

#[test]
fn test_points_at_infinity() {
    // Two lines with identical slope: y = 2x + 1 and y = 2x + 5
    // 2x - y + 1 = 0 -> (2, -1, 1)
    // 2x - y + 5 = 0 -> (2, -1, 5)
    let l1 = Line3::new(2, -1, 1);
    let l2 = Line3::new(2, -1, 5);

    let intersection = meet(l1, l2);
    assert!(intersection.is_infinite());
    assert_eq!(intersection.w(), 0);

    // Direction vector of intersection must point along the line direction (dx=1, dy=2)
    let (dx, dy) = (intersection.x(), intersection.y());
    // Cross product (dx, dy) with (1, 2) must be zero
    assert_eq!(dx as i64 * 2 - dy as i64, 0);
}

#[test]
fn test_matrix_composition() {
    let t1 = Matrix3x3::translation(10, 20);
    let t2 = Matrix3x3::translation(-5, 15);
    let combined = t1.multiply(&t2);

    let pt = Coord3::point(0, 0);
    let moved = combined.transform_coord(pt);

    assert_eq!(moved.to_euclidean(), Some((5, 35)));
}

#[test]
fn test_pinhole_projection_perspective_shrinkage() {
    const PROJ: Matrix4x4 = Matrix4x4::perspective(200, 160, 120, 10, 1000);

    // Two offset points: one close, one far
    let v_near = Coord4::point(100, 0, 100);
    let v_far = Coord4::point(100, 0, 400);

    // Verify 3D depth comparison for painter's algorithm
    assert_eq!(v_near.compare_depth(&v_far), core::cmp::Ordering::Less);

    let (p_near, clip_near) = PROJ.project_to_screen(v_near);
    let (p_far, clip_far) = PROJ.project_to_screen(v_far);

    // Retained depth Z' is preserved for sorting
    assert_eq!(clip_near.compare_depth(&clip_far), core::cmp::Ordering::Less);

    let (x_near, _) = p_near.to_euclidean().unwrap();
    let (x_far, _) = p_far.to_euclidean().unwrap();

    // Farther object projects closer to the optical center (160)
    assert!(x_near > x_far);
    assert_eq!(x_near, 360); // 160 + 200 * 100 / 100 = 360
    assert_eq!(x_far, 210);  // 160 + 200 * 100 / 400 = 210
}

#[test]
fn test_conic_ellipse_incremental_stepper() {
    let ellipse = ConicMatrix::ellipse(50, 50, 30, 20);

    // Test multiple scanlines
    for y in 25..=75 {
        let mut stepper = ellipse.scanline_stepper(10, y);
        for x in 10..=90 {
            let direct = ellipse.evaluate_pixel(x, y);
            assert_eq!(stepper.current_q, direct, "Failed at ({x}, {y})");
            assert_eq!(stepper.is_inside(), direct <= 0);
            stepper.advance();
        }
    }
}
