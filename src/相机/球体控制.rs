use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::math::Vec3;
use bevy::prelude::*;

// 这个组件标记球体根节点，拖拽旋转时直接修改它的旋转角度。
#[derive(Component)]
pub struct 球体根;

// 这个组件标记用于观察地球的相机，滚轮缩放时沿着射线移动相机。
#[derive(Component)]
pub struct 球体相机;

// 球体半径，和场景中的地球尺寸保持一致。
pub const 球体半径: f32 = 10000.0;

// 真实地球半径（米），用于把世界单位换算成米。
const 真实地球半径米: f64 = 6_378_137.0;
// 每 1 个世界单位对应多少米。
const 米每单位: f64 = 真实地球半径米 / 球体半径 as f64;
// 允许的最小离地高度：10 米换算成世界单位，作为滚轮缩放的夹取下限。
// 相机下降到离地 10 米后不再继续放大，太贴近地面会因浮点精度导致缩放卡死。
const 最小离地: f32 = (10.0 / 米每单位) as f32;

// 这个系统把拖拽旋转和滚轮缩放整合进同一轮更新里，避免两个系统同时访问同一相机 Transform。
pub fn 控制_地球_视角(
    // 获取鼠标按钮状态，判断是否按住左键。
    鼠标: Res<ButtonInput<MouseButton>>,
    // 读取鼠标移动消息，获取拖拽增量。
    mut 鼠标移动: MessageReader<MouseMotion>,
    // 读取滚轮消息，获取缩放方向。
    mut 滚轮: MessageReader<MouseWheel>,
    // 获取窗口信息，用于读取视口高度和光标位置。
    窗口查询: Query<&Window>,
    // 把相机和球体根的 Transform 查询分开，避免 Bevy 在同一系统中出现重叠互斥访问。
    mut 控制集: ParamSet<
        (
            Query<
                (&Camera, &GlobalTransform, &Projection, &mut Transform),
                With<球体相机>,
            >,
            Query<&mut Transform, With<球体根>>,
        ),
    >,
) {
    let Ok(窗口) = 窗口查询.single() else {
        return;
    };
    let 视口高 = 窗口.height();
    let 光标 = 窗口.cursor_position();

    let mut 鼠标增量 = Vec2::ZERO;
    for 事件 in 鼠标移动.read() {
        鼠标增量 += 事件.delta;
    }

    // 先处理相机和缩放逻辑，确保该查询在作用域结束时被释放，再进入球体旋转查询。
    let 旋转数据 = {
        let mut 相机查询 = 控制集.p0();
        let Ok((相机, _全局变换, 投影, mut 相机变换)) = 相机查询.single_mut() else {
            return;
        };

        // 跟手换算：屏幕中心处的球面点离相机的深度就是离地距离，
        // 该深度下每 1 像素对应的世界长度 = 2 * 离地 * tan(fov/2) / 视口高，
        // 与 地球信息 的“米每像素”同源，保证拖拽距离和鼠标像素严格一致。
        let fov = match 投影 {
            Projection::Perspective(p) => p.fov,
            _ => 45.0_f32.to_radians(),
        };
        let 相机距离 = 相机变换.translation.length();
        let 离地距离 = (相机距离 - 球体半径).max(0.0);
        let 每像素世界长 = 2.0 * 离地距离 * (fov * 0.5).tan() / 视口高.max(1.0);

        let 旋转参数 = if 鼠标.pressed(MouseButton::Left) && 鼠标增量 != Vec2::ZERO {
            // 垂直拖拽绕“屏幕右方”轴旋转，屏幕中心点到该轴的杠杆恒为球体半径。
            let 垂直灵敏度 = 每像素世界长 / 球体半径;
            // 水平拖拽绕世界极轴 Y 旋转，屏幕中心点到极轴的杠杆是 球体半径 * cos(纬度)，
            // 除以该 cos 才能让相机处于高纬视角时水平位移也严格跟手。
            let 相机方向 = 相机变换.translation.normalize_or_zero();
            let cos纬 = (相机方向.x * 相机方向.x + 相机方向.z * 相机方向.z)
                .sqrt()
                .max(1e-3);
            let 水平灵敏度 = 每像素世界长 / (球体半径 * cos纬);

            let 水平角度 = 鼠标增量.x * 水平灵敏度;
            let 垂直角度 = 鼠标增量.y * 垂直灵敏度;
            let 水平旋转 = Quat::from_rotation_y(水平角度);
            let 右方 = 相机变换.translation.cross(Vec3::Y).normalize_or_zero();
            let 垂直旋转 = Quat::from_axis_angle(右方, -垂直角度);
            Some((水平旋转, 垂直旋转))
        } else {
            None
        };

        // 缩放锚定：累计本次所有滚轮事件需要的地球旋转，
        // 使指针下的球面点在缩放前后停留在同一像素处。
        let mut 缩放旋转 = Quat::IDENTITY;

        for 事件 in 滚轮.read() {
            // 只调整相机和地球中心的距离，不让地球本体在屏幕上位移。
            let 当前距离 = 相机变换.translation.length();
            let 视线方向 = 相机变换
                .translation
                .normalize_or_zero();

            // 如果相机已经在中心附近，说明还没建立有效视角，则直接忽略本次缩放。
            if 视线方向 == Vec3::ZERO {
                continue;
            }

            let 放大倍率 = 0.92_f32;
            let 缩小倍率 = 1.08_f32;
            let 倍率 = if 事件.y > 0.0 { 放大倍率 } else { 缩小倍率 };
            let 当前离地 = (当前距离 - 球体半径).max(最小离地);
            let 新离地 = (当前离地 * 倍率).clamp(最小离地, 50000.0);
            let 新距离 = 球体半径 + 新离地;

            // 缩放前指针射线命中的球面点（单位向量），用于缩放后的锚定校正。
            let 缩放前命中 = 光标
                .and_then(|位置| 指针球面点(&相机, &相机变换, 位置));

            // 关键修正：相机沿着“从地心指向相机”的方向移动，但永远保持朝向地心，
            // 因此地球始终位于屏幕正中，不会因缩放而偏移。
            相机变换.translation = 视线方向 * 新距离;
            相机变换.look_at(Vec3::ZERO, Vec3::Y);

            if let (Some(起点), Some(位置)) = (缩放前命中, 光标) {
                // 同一像素的射线在相机径向后移后指向球面另一处，
                // 把地球旋转“起点→终点”的最短弧，该球面点便回到指针下方。
                if let Some(终点) = 指针球面点或切点(&相机, &相机变换, 位置) {
                    缩放旋转 = Quat::from_rotation_arc(起点, 终点) * 缩放旋转;
                }
            }
        }

        (旋转参数, 缩放旋转)
    };

    let (旋转参数, 缩放旋转) = 旋转数据;
    // 拖拽旋转按原有方式叠加，缩放锚定旋转额外左乘，两者都是世界系旋转。
    let 拖拽旋转 = 旋转参数.map(|(水平旋转, 垂直旋转)| 垂直旋转 * 水平旋转);
    let 总旋转 = 缩放旋转 * 拖拽旋转.unwrap_or(Quat::IDENTITY);

    if 总旋转 != Quat::IDENTITY {
        let mut 根查询 = 控制集.p1();
        let Ok(mut 根变换) = 根查询.single_mut() else {
            return;
        };
        根变换.rotation = 总旋转 * 根变换.rotation;
    }
}

// 求光标像素射线与球面的最近交点，返回该点的单位向量（地心指向交点）。
// 与 地球信息 的射线拾取使用同一半径、同一求交公式，保证锚定点与 HUD 经纬度一致。
fn 指针球面点(相机: &Camera, 相机变换: &Transform, 光标: Vec2) -> Option<Vec3> {
    let 全局 = GlobalTransform::from(*相机变换);
    let Ok(射线) = 相机.viewport_to_world(&全局, 光标) else {
        return None;
    };
    let 原点 = 射线.origin;
    let 方向 = 射线.direction.as_vec3();
    let b = 原点.dot(方向);
    let c = 原点.length_squared() - 球体半径 * 球体半径;
    let 判别式 = b * b - c;
    if 判别式 < 0.0 {
        return None;
    }
    let t = (-b - 判别式.sqrt()).max(0.0);
    Some((原点 + 方向 * t).normalize())
}

// 同 指针球面点，但缩小后指针可能落在地球盘面之外（射线不再相交），
// 此时取射线到地心的垂足方向（切点）作为交点的极限位置，让锚定校正保持连续。
fn 指针球面点或切点(相机: &Camera, 相机变换: &Transform, 光标: Vec2) -> Option<Vec3> {
    let 全局 = GlobalTransform::from(*相机变换);
    let Ok(射线) = 相机.viewport_to_world(&全局, 光标) else {
        return None;
    };
    let 原点 = 射线.origin;
    let 方向 = 射线.direction.as_vec3();
    let b = 原点.dot(方向);
    let c = 原点.length_squared() - 球体半径 * 球体半径;
    let 判别式 = b * b - c;
    let t = if 判别式 >= 0.0 { -b - 判别式.sqrt() } else { -b };
    Some((原点 + 方向 * t).normalize())
}
