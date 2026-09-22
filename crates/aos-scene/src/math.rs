//! Minimal f32 linear algebra for ADR 0011 SceneGraph math.

use serde::{Deserialize, Serialize};

pub const EPSILON: f32 = 1e-5;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };
    pub const UNIT_X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    pub const UNIT_Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const UNIT_Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn from_array(a: [f32; 3]) -> Self {
        Self {
            x: a[0],
            y: a[1],
            z: a[2],
        }
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn normalized(self) -> Option<Self> {
        let len = self.length();
        if len < EPSILON {
            None
        } else {
            Some(Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            })
        }
    }

    pub fn cross(self, rhs: Self) -> Self {
        Self {
            x: self.y * rhs.z - self.z * rhs.y,
            y: self.z * rhs.x - self.x * rhs.z,
            z: self.x * rhs.y - self.y * rhs.x,
        }
    }

    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

impl std::ops::Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}

/// Unit quaternion stored as **`[x, y, z, w]`** (ADR 0011).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    pub fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    pub fn from_array(a: [f32; 4]) -> Self {
        Self {
            x: a[0],
            y: a[1],
            z: a[2],
            w: a[3],
        }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt()
    }

    pub fn normalized(self) -> Option<Self> {
        let len = self.length();
        if !len.is_finite() || len < EPSILON {
            None
        } else {
            Some(Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
                w: self.w / len,
            })
        }
    }

    /// Axis-angle; `axis` need not be unit; `angle` in **radians**.
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let axis = axis.normalized().unwrap_or(Vec3::UNIT_Y);
        let half = angle * 0.5;
        let s = half.sin();
        Self {
            x: axis.x * s,
            y: axis.y * s,
            z: axis.z * s,
            w: half.cos(),
        }
        .normalized()
        .unwrap_or(Self::IDENTITY)
    }

    /// Shortest rotation taking unit vector `from` onto unit vector `to`.
    pub fn from_rotation_arc(from: Vec3, to: Vec3) -> Self {
        let from = from.normalized().unwrap_or(Vec3::UNIT_Y);
        let to = to.normalized().unwrap_or(Vec3::UNIT_Y);
        let dot = from.dot(to).clamp(-1.0, 1.0);
        if dot > 1.0 - EPSILON {
            return Self::IDENTITY;
        }
        if dot < -1.0 + EPSILON {
            // 180°: pick an orthogonal axis.
            let axis = if from.x.abs() < 0.9 {
                from.cross(Vec3::UNIT_X)
            } else {
                from.cross(Vec3::UNIT_Y)
            }
            .normalized()
            .unwrap_or(Vec3::UNIT_Z);
            return Self::from_axis_angle(axis, std::f32::consts::PI);
        }
        let axis = from.cross(to);
        let w = 1.0 + dot;
        Self::from_xyzw(axis.x, axis.y, axis.z, w)
            .normalized()
            .unwrap_or(Self::IDENTITY)
    }

    pub fn conjugate(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: self.w,
        }
    }

    /// Extract rotation from an affine matrix (orthonormalize columns).
    pub fn from_mat4_rotation(m: Mat4) -> Self {
        let c0 = Vec3::new(m.m[0], m.m[1], m.m[2])
            .normalized()
            .unwrap_or(Vec3::UNIT_X);
        let c1 = Vec3::new(m.m[4], m.m[5], m.m[6])
            .normalized()
            .unwrap_or(Vec3::UNIT_Y);
        let c2 = Vec3::new(m.m[8], m.m[9], m.m[10])
            .normalized()
            .unwrap_or(Vec3::UNIT_Z);
        // Shepperd's method from rotation matrix columns.
        let trace = c0.x + c1.y + c2.z;
        if trace > 0.0 {
            let s = (trace + 1.0).sqrt() * 2.0;
            Self::from_xyzw(
                (c1.z - c2.y) / s,
                (c2.x - c0.z) / s,
                (c0.y - c1.x) / s,
                0.25 * s,
            )
            .normalized()
            .unwrap_or(Self::IDENTITY)
        } else if c0.x > c1.y && c0.x > c2.z {
            let s = (1.0 + c0.x - c1.y - c2.z).sqrt() * 2.0;
            Self::from_xyzw(
                0.25 * s,
                (c0.y + c1.x) / s,
                (c2.x + c0.z) / s,
                (c1.z - c2.y) / s,
            )
            .normalized()
            .unwrap_or(Self::IDENTITY)
        } else if c1.y > c2.z {
            let s = (1.0 + c1.y - c0.x - c2.z).sqrt() * 2.0;
            Self::from_xyzw(
                (c0.y + c1.x) / s,
                0.25 * s,
                (c1.z + c2.y) / s,
                (c2.x - c0.z) / s,
            )
            .normalized()
            .unwrap_or(Self::IDENTITY)
        } else {
            let s = (1.0 + c2.z - c0.x - c1.y).sqrt() * 2.0;
            Self::from_xyzw(
                (c2.x + c0.z) / s,
                (c1.z + c2.y) / s,
                0.25 * s,
                (c0.y - c1.x) / s,
            )
            .normalized()
            .unwrap_or(Self::IDENTITY)
        }
    }

    pub fn rotate_vec(self, v: Vec3) -> Vec3 {
        // q * (0,v) * q^{-1}
        let qv = Vec3::new(self.x, self.y, self.z);
        let uv = qv.cross(v);
        let uuv = qv.cross(uv);
        v + (uv * (2.0 * self.w)) + (uuv * 2.0)
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }
}

impl std::ops::Mul for Quat {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self {
            x: self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            y: self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            z: self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
            w: self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
        }
        .normalized()
        .unwrap_or(Self::IDENTITY)
    }
}

/// Column-major 4×4 matrix (`M * v` with column vectors).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// Column-major storage: index `col * 4 + row`.
    pub m: [f32; 16],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        m: [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ],
    };

    pub fn from_cols(c0: [f32; 4], c1: [f32; 4], c2: [f32; 4], c3: [f32; 4]) -> Self {
        let mut m = [0.0; 16];
        m[..4].copy_from_slice(&c0);
        m[4..8].copy_from_slice(&c1);
        m[8..12].copy_from_slice(&c2);
        m[12..16].copy_from_slice(&c3);
        Self { m }
    }

    pub fn transform_point(self, p: Vec3) -> Vec3 {
        let x = self.m[0] * p.x + self.m[4] * p.y + self.m[8] * p.z + self.m[12];
        let y = self.m[1] * p.x + self.m[5] * p.y + self.m[9] * p.z + self.m[13];
        let z = self.m[2] * p.x + self.m[6] * p.y + self.m[10] * p.z + self.m[14];
        let w = self.m[3] * p.x + self.m[7] * p.y + self.m[11] * p.z + self.m[15];
        if w.abs() < EPSILON {
            Vec3::new(x, y, z)
        } else {
            Vec3::new(x / w, y / w, z / w)
        }
    }

    pub fn transform_vector(self, v: Vec3) -> Vec3 {
        Vec3::new(
            self.m[0] * v.x + self.m[4] * v.y + self.m[8] * v.z,
            self.m[1] * v.x + self.m[5] * v.y + self.m[9] * v.z,
            self.m[2] * v.x + self.m[6] * v.y + self.m[10] * v.z,
        )
    }

    /// Affine inverse for TRS matrices (last row ≈ `[0,0,0,1]`).
    pub fn try_inverse_affine(self) -> Option<Self> {
        // Invert 3×3 (columns = basis * scale).
        let a = self.m[0];
        let b = self.m[4];
        let c = self.m[8];
        let d = self.m[1];
        let e = self.m[5];
        let f = self.m[9];
        let g = self.m[2];
        let h = self.m[6];
        let i = self.m[10];
        let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if !det.is_finite() || det.abs() < EPSILON {
            return None;
        }
        let inv_det = 1.0 / det;
        let r00 = (e * i - f * h) * inv_det;
        let r01 = (c * h - b * i) * inv_det;
        let r02 = (b * f - c * e) * inv_det;
        let r10 = (f * g - d * i) * inv_det;
        let r11 = (a * i - c * g) * inv_det;
        let r12 = (c * d - a * f) * inv_det;
        let r20 = (d * h - e * g) * inv_det;
        let r21 = (b * g - a * h) * inv_det;
        let r22 = (a * e - b * d) * inv_det;
        let tx = self.m[12];
        let ty = self.m[13];
        let tz = self.m[14];
        let itx = -(r00 * tx + r01 * ty + r02 * tz);
        let ity = -(r10 * tx + r11 * ty + r12 * tz);
        let itz = -(r20 * tx + r21 * ty + r22 * tz);
        Some(Self::from_cols(
            [r00, r10, r20, 0.0],
            [r01, r11, r21, 0.0],
            [r02, r12, r22, 0.0],
            [itx, ity, itz, 1.0],
        ))
    }

    /// TRS: scale, then rotate, then translate (`T * R * S`).
    pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        let r = rotation;
        // Rotation matrix from quaternion (column-major).
        let xx = r.x * r.x;
        let yy = r.y * r.y;
        let zz = r.z * r.z;
        let xy = r.x * r.y;
        let xz = r.x * r.z;
        let yz = r.y * r.z;
        let wx = r.w * r.x;
        let wy = r.w * r.y;
        let wz = r.w * r.z;

        let c0 = [
            (1.0 - 2.0 * (yy + zz)) * scale.x,
            (2.0 * (xy + wz)) * scale.x,
            (2.0 * (xz - wy)) * scale.x,
            0.0,
        ];
        let c1 = [
            (2.0 * (xy - wz)) * scale.y,
            (1.0 - 2.0 * (xx + zz)) * scale.y,
            (2.0 * (yz + wx)) * scale.y,
            0.0,
        ];
        let c2 = [
            (2.0 * (xz + wy)) * scale.z,
            (2.0 * (yz - wx)) * scale.z,
            (1.0 - 2.0 * (xx + yy)) * scale.z,
            0.0,
        ];
        let c3 = [translation.x, translation.y, translation.z, 1.0];
        Self::from_cols(c0, c1, c2, c3)
    }
}

impl std::ops::Mul for Mat4 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        let mut out = [0.0; 16];
        for col in 0..4 {
            for row in 0..4 {
                let mut s = 0.0;
                for k in 0..4 {
                    s += self.m[k * 4 + row] * rhs.m[col * 4 + k];
                }
                out[col * 4 + row] = s;
            }
        }
        Self { m: out }
    }
}
