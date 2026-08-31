use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::球体控制::球体相机;

// 这里与主场景中的球体半径保持一致，保证鼠标落点经纬度和离地高度计算正确。
pub const 地球半径: f32 = 10_000.0;
pub const 真实地球半径米: f64 = 6_378_137.0;
pub const 米每单位: f64 = 真实地球半径米 / 地球半径 as f64;

#[derive(Component)]
pub struct 地球信息文本;

#[derive(Resource, Default)]
pub struct 地理状态 {
    pub 鼠标经度: Option<f64>,
    pub 鼠标纬度: Option<f64>,
    pub 鼠标海拔米: f64,
    pub 相机离地米: f64,
    pub 米每像素: f64,
}

// 读取系统中可用的 CJK 字体，保证中文显示不出现方块字。
pub fn 读取中文字体(字体仓库: &mut ResMut<Assets<Font>>) -> Handle<Font> {
    let 路径 = "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc";
    let 字节 = std::fs::read(路径).unwrap_or_else(|_| {
        // Debian KDE 常用的备选字体路径，避免 Noto 路径不存在时直接失败。
        std::fs::read("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc")
            .expect("未找到可用中文字体：Noto Sans CJK 或文泉驿正黑")
    });
    字体仓库.add(Font::from_bytes(字节))
}

pub fn 球面经纬度(坐标: [f64; 3]) -> 经纬度 {
    let 半径 = (坐标[0] * 坐标[0] + 坐标[1] * 坐标[1] + 坐标[2] * 坐标[2]).sqrt();
    经纬度 {
        经度: 归一化经度((-坐标[2]).atan2(坐标[0]).to_degrees()),
        纬度: (坐标[1] / 半径).clamp(-1.0, 1.0).asin().to_degrees(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct 经纬度 {
    pub 经度: f64,
    pub 纬度: f64,
}

pub fn 归一化经度(经度: f64) -> f64 {
    (经度 + 180.0).rem_euclid(360.0) - 180.0
}

pub fn 初始化地球信息(mut 命令: Commands, mut 字体_仓库: ResMut<Assets<Font>>) {
    let 中文字体 = 读取中文字体(&mut 字体_仓库);

    命令.spawn((
        Text::new("经度: --\n纬度: --\n离地高度: -- 米"),
        TextFont::from_font_size(24.0).with_font(中文字体.clone()),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(20.0),
            top: Val::Px(20.0),
            ..default()
        },
        地球信息文本,
    ));
}

pub fn 更新地球信息(
    窗口查询: Query<&Window, With<PrimaryWindow>>,
    相机查询: Query<(&Camera, &GlobalTransform, &Projection), With<球体相机>>,
    mut 地理: ResMut<地理状态>,
    mut 文本查询: Query<&mut Text, With<地球信息文本>>,
) {
    let Ok(窗口) = 窗口查询.single() else {
        return;
    };
    let Ok((相机, 变换, 投影)) = 相机查询.single() else {
        return;
    };

    let 离地单位 = (变换.translation().length() - 地球半径).max(0.0);
    地理.相机离地米 = f64::from(离地单位) * 米每单位;

    let fov = match 投影 {
        Projection::Perspective(p) => p.fov,
        _ => 60.0_f32.to_radians(),
    };
    let 视口高 = 窗口.height();
    if 视口高 > 0.0 && 离地单位 > 0.001 {
        let 可见世界高 = 2.0 * f64::from(离地单位) * f64::from((fov / 2.0).tan());
        let 可见米高 = 可见世界高 * 米每单位;
        地理.米每像素 = 可见米高 / f64::from(视口高);
    } else {
        地理.米每像素 = 0.0;
    }

    let Some(光标) = 窗口.cursor_position() else {
        地理.鼠标经度 = None;
        地理.鼠标纬度 = None;
        return;
    };
    let Ok(射线) = 相机.viewport_to_world(变换, 光标) else {
        return;
    };
    let 原点 = 射线.origin;
    let 方向 = 射线.direction.as_vec3();
    let b = 原点.dot(方向);
    let c = 原点.length_squared() - 地球半径.powi(2);
    let 判别式 = b * b - c;
    if 判别式 < 0.0 {
        地理.鼠标经度 = None;
        地理.鼠标纬度 = None;
        return;
    }
    let t = (-b - 判别式.sqrt()).max(0.0);
    let 命中 = 原点 + 方向 * t;
    let 位置 = 球面经纬度([f64::from(命中.x), f64::from(命中.y), f64::from(命中.z)]);
    地理.鼠标经度 = Some(位置.经度);
    地理.鼠标纬度 = Some(位置.纬度);
    地理.鼠标海拔米 = 0.0;

    let Ok(mut 文本) = 文本查询.single_mut() else {
        return;
    };
    let 经度 = 地理.鼠标经度.map(|值| format!("经度: {值:.5}°")).unwrap_or_else(|| "经度: --".to_string());
    let 纬度 = 地理.鼠标纬度.map(|值| format!("纬度: {值:.5}°")).unwrap_or_else(|| "纬度: --".to_string());
    let 高度 = format!("离地高度: {:.0} 米", 地理.相机离地米);
    文本.0 = format!("{经度}\n{纬度}\n{高度}");
}
