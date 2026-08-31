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
            Query<(&Camera, &GlobalTransform, &mut Transform), With<球体相机>>,
            Query<&mut Transform, With<球体根>>,
        ),
    >,
) {
    let Ok(窗口) = 窗口查询.single() else {
        return;
    };
    let 视口高 = 窗口.height();
    let _当前光标 = 窗口.cursor_position();

    let mut 鼠标增量 = Vec2::ZERO;
    for 事件 in 鼠标移动.read() {
        鼠标增量 += 事件.delta;
    }

    // 先处理相机和缩放逻辑，确保该查询在作用域结束时被释放，再进入球体旋转查询。
    let 旋转数据 = {
        let mut 相机查询 = 控制集.p0();
        let Ok((_相机, _全局变换, mut 相机变换)) = 相机查询.single_mut() else {
            return;
        };

        let 焦距 = 1.0_f32;
        let 相机距离 = 相机变换.translation.length();
        let 离地距离 = (相机距离 - 球体半径).max(0.0);
        let 基础灵敏度 = 离地距离 / (焦距 * 球体半径 * 视口高 * 0.5);
        let 像素灵敏度 = 基础灵敏度 * 0.8;

        let 旋转参数 = if 鼠标.pressed(MouseButton::Left) && 鼠标增量 != Vec2::ZERO {
            let 窗口旋转 = 鼠标增量;
            let 水平角度 = 窗口旋转.x * 像素灵敏度;
            let 垂直角度 = 窗口旋转.y * 像素灵敏度;
            let 水平旋转 = Quat::from_rotation_y(水平角度);
            let 右方 = 相机变换.translation.cross(Vec3::Y).normalize_or_zero();
            let 垂直旋转 = Quat::from_axis_angle(右方, -垂直角度);
            Some((水平旋转, 垂直旋转))
        } else {
            None
        };

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

            // 关键修正：相机沿着“从地心指向相机”的方向移动，但永远保持朝向地心。
            相机变换.translation = 视线方向 * 新距离;
            相机变换.look_at(Vec3::ZERO, Vec3::Y);
        }

        旋转参数
    };

    if let Some((水平旋转, 垂直旋转)) = 旋转数据 {
        let mut 根查询 = 控制集.p1();
        let Ok(mut 根变换) = 根查询.single_mut() else {
            return;
        };
        根变换.rotation = 垂直旋转 * 水平旋转 * 根变换.rotation;
    }
}
