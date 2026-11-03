//! `C_car` chase helpers from mafia-re PR3 (`pr3` 25454ec).
//!
//! Game.exe SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! `C_car::GetAIPos` 0x429510 and `C_car::GetNavigationPoints` 0x4299c0.
//! Both are pure projections over caller-owned state: the model points at
//! vehicle+0x2010/+0x201c, the world cache at +0x2028/+0x2034, the model
//! matrix, `pivot_pos_2` and `vehicle_size_2`. No actor is constructed here
//! and no global is touched; the caller selects the cache slot with `flag`.
//!
//! `S_vector::operator*` association matches the existing
//! `vehicle_car_body_dynamic::transform` order (Z,Y,X for X; Z,X,Y for Y/Z).

/// Row-vector transform of a model point by a 4x4 model matrix.
///
/// `matrix` rows are `[m00..m03, m10..m13, m20..m23, m30..m33]`; the
/// translation row is `matrix[3]`.
pub fn transform_model_point(point: [f32; 3], matrix: [[f32; 4]; 4]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for column in 0..3 {
        // Z,Y,X for X; Z,X,Y for Y/Z, matching the original operator*.
        let [a, d, e] = if column == 0 { [2, 1, 0] } else { [2, 0, 1] };
        out[column] = matrix[a][column] * point[a]
            + matrix[d][column] * point[d]
            + matrix[e][column] * point[e]
            + matrix[3][column];
    }
    out
}

/// Original `C_car::GetAIPos`: with `flag` set transforms `point_true`
/// (vehicle+0x2010) into the +0x2028 cache, otherwise transforms
/// `point_false` (vehicle+0x201c) into +0x2034. Returns the selected world
/// point; the caller owns both caches.
pub fn get_ai_pos(
    flag: bool,
    point_true: [f32; 3],
    point_false: [f32; 3],
    matrix: [[f32; 4]; 4],
) -> [f32; 3] {
    if flag {
        transform_model_point(point_true, matrix)
    } else {
        transform_model_point(point_false, matrix)
    }
}

/// Original `C_car::GetNavigationPoints` 0x4299c0: the four corners of the
/// car's footprint. `inflate` is the script-supplied margin, `model_y` is the
/// +0x2010 Y word, `pivot` is `pivot_pos_2` (X/Z used) and `size` is
/// `vehicle_size_2` (X/Z used).
pub fn navigation_points(
    inflate: f32,
    model_y: f32,
    pivot: [f32; 3],
    size: [f32; 3],
) -> [[f32; 3]; 4] {
    let min_x = -(inflate + pivot[0]);
    let max_x = size[0] - pivot[0] + inflate;
    let min_z = -(inflate + pivot[2]);
    let max_z = size[2] - pivot[2] + inflate;
    [
        [min_x, model_y, max_z],
        [max_x, model_y, max_z],
        [min_x, model_y, min_z],
        [max_x, model_y, min_z],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> [[f32; 4]; 4] {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    #[test]
    fn identity_matrix_returns_model_points() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        assert_eq!(get_ai_pos(true, a, b, identity()), a);
        assert_eq!(get_ai_pos(false, a, b, identity()), b);
    }

    #[test]
    fn translation_matrix_matches_row_vector_order() {
        let matrix = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [10.0, 20.0, 30.0, 1.0],
        ];
        assert_eq!(
            get_ai_pos(true, [1.0, 2.0, 3.0], [0.0; 3], matrix),
            [11.0, 22.0, 33.0]
        );
    }

    #[test]
    fn navigation_corners_follow_original_formula() {
        let points = navigation_points(0.5, 1.0, [2.0, 0.0, 3.0], [6.0, 0.0, 8.0]);
        assert_eq!(
            points,
            [
                [-2.5, 1.0, 5.5],
                [4.5, 1.0, 5.5],
                [-2.5, 1.0, -3.5],
                [4.5, 1.0, -3.5],
            ]
        );
    }

    #[test]
    fn zero_inflate_collapses_to_pivot_size_box() {
        let points = navigation_points(0.0, 0.0, [0.0, 0.0, 0.0], [4.0, 0.0, 6.0]);
        assert_eq!(
            points,
            [
                [0.0, 0.0, 6.0],
                [4.0, 0.0, 6.0],
                [0.0, 0.0, 0.0],
                [4.0, 0.0, 0.0],
            ]
        );
    }
}
