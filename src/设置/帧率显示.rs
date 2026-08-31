use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

#[derive(Component)]
pub struct 帧率文本;

pub fn 初始化帧率显示(mut 命令: Commands) {
    // 创建一个 UI 文本实体，放置在窗口右上角，用来显示实时 FPS。
    // 在 Bevy 0.19 中，UI 文本直接使用 Text + TextFont + TextColor + Style，
    // 不再依赖旧版的 TextBundle。这样更符合新版本的 ECS 组织方式。
    命令.spawn((
        Text::new("FPS: 0"),
        TextFont::from_font_size(28.0),
        TextColor(Color::WHITE),
        Node {
            // 使用绝对定位，确保文本固定在窗口右上角。
            position_type: PositionType::Absolute,
            // 距离窗口顶部 20 像素。
            top: Val::Px(20.0),
            // 距离窗口右侧 20 像素。
            right: Val::Px(20.0),
            ..default()
        },
        帧率文本,
    ));
}

pub fn 更新帧率文本(
    // 诊断存储，包含当前的帧率、平均帧时间等统计信息。
    诊断: Res<DiagnosticsStore>,
    // 查询所有带有 帧率文本 组件的文本对象。
    mut 查询文本: Query<&mut Text, With<帧率文本>>,
) {
    // 从诊断数据中读取 FPS 数值，若没有数据则默认返回 0。
    let 帧率 = 诊断
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|数值| 数值.value())
        .unwrap_or(0.0);

    // 遍历所有 FPS 文本实体，并更新为当前帧率值。
    for mut 文本 in &mut 查询文本 {
        // Bevy 0.19 中 Text 作为单个字符串容器，直接替换其 .0 字段即可。
        文本.0 = format!("FPS: {:.0}", 帧率);
    }
}
