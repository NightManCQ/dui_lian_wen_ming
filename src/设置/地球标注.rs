use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::sprite::Text2d;

// 北极标签实体，用于唯一标识并更新北极文字。
#[derive(Component)]
pub struct 北极标签;

// 南极标签实体，用于唯一标识并更新南极文字。
#[derive(Component)]
pub struct 南极标签;

// 负责在地球表面绘制经纬度线圈，并在两极上方标注“北”和“南”。
pub fn 生成_地球标注(
    // 命令对象，用于创建几何线和文字标签实体。
    命令: &mut Commands,
    // 网格资源仓库，用于生成真正的三维线网格，并确保它在渲染中可见。
    网格_仓库: &mut ResMut<Assets<Mesh>>,
    // 材质资源仓库，用于给线网格指定淡白色透明材质，避免遮挡过强。
    材质_仓库: &mut ResMut<Assets<StandardMaterial>>,
    // 字体资源仓库，用于装载真实字体，使文本内容能真正渲染出来。
    字体_仓库: &mut ResMut<Assets<Font>>,
    // 需要将标注挂到的地球根节点，保证它们和地球一起旋转。
    根实体: Entity,
) {
    // 地球基准半径：线圈按此半径绘制，勾勒出线框地球的轮廓，瓦片贴在略高于它的位置。
    let 半径 = 10000.0;
    let mut 位置集合 = Vec::new();
    let mut 索引集合 = Vec::new();

    // 先生成纬线：按固定纬度圈出一组横向圆环。
    for 线圈编号 in -6..=6 {
        let 纬度角度 = (线圈编号 as f32 * 15.0_f32).to_radians();
        let y = 半径 * 纬度角度.sin();
        let 平面半径 = 半径 * 纬度角度.cos();

        let 起始索引 = 位置集合.len() as u32;
        for 角度编号 in 0..=72 {
            let 经度角度 = (角度编号 as f32 / 72.0) * std::f32::consts::TAU;
            let x = 平面半径 * 经度角度.cos();
            let z = 平面半径 * 经度角度.sin();
            位置集合.push([x, y, z]);
        }

        for 角度编号 in 0..72 {
            let a = 起始索引 + 角度编号 as u32;
            let b = 起始索引 + (角度编号 as u32 + 1u32) % 73u32;
            索引集合.push(a);
            索引集合.push(b);
        }
    }

    // 再生成经线：按固定经度从南极到北极画一组竖线。
    for 线圈编号 in 0..=11 {
        let 经度角度 = (线圈编号 as f32 / 12.0) * std::f32::consts::TAU;
        let 起始索引 = 位置集合.len() as u32;

        for 角度编号 in 0..=72 {
            let 纬度角度 = (角度编号 as f32 / 72.0 - 0.5) * std::f32::consts::PI;
            let 纬度半径 = 半径 * 纬度角度.cos();
            let x = 纬度半径 * 经度角度.cos();
            let y = 半径 * 纬度角度.sin();
            let z = 纬度半径 * 经度角度.sin();
            位置集合.push([x, y, z]);
        }

        for 角度编号 in 0..72 {
            let a = 起始索引 + 角度编号 as u32;
            let b = 起始索引 + (角度编号 as u32 + 1u32) % 73u32;
            索引集合.push(a);
            索引集合.push(b);
        }
    }

    // 创建真正的线网格 Mesh，使用 LineList 拓扑，确保能直接以线段方式绘制。
    let mut 线网格 = Mesh::new(
        PrimitiveTopology::LineList,
        bevy::asset::RenderAssetUsages::all(),
    );
    线网格.insert_attribute(Mesh::ATTRIBUTE_POSITION, 位置集合);
    线网格.insert_indices(Indices::U32(索引集合));

    let 线材质 = 材质_仓库.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.42),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    // 加载项目内的真实字体，这样中文字符才可以被绘制到屏幕上。
    let 字体 = 字体_仓库.add(Font::from_bytes(include_bytes!("../../assets/fonts/DejaVuSans.ttf").to_vec()));

    // 把线网格挂到地球根节点下，和地球本体一起旋转。
    命令.entity(根实体).with_children(|子命令| {
        子命令.spawn((
            Mesh3d(网格_仓库.add(线网格)),
            MeshMaterial3d(线材质),
            Transform::IDENTITY,
        ));

        // 在北极上方添加“北”文字标注，表示北极位置，并跟随地球旋转。
        子命令.spawn((
            Text2d::new("北"),
            TextFont::from_font_size(700.0).with_font(字体.clone()),
            TextColor(Color::WHITE),
            Transform::from_xyz(0.0, 半径 + 3000.0, 3000.0),
            北极标签,
        ));

        // 在南极上方添加“南”文字标注，表示南极位置，并跟随地球旋转。
        子命令.spawn((
            Text2d::new("南"),
            TextFont::from_font_size(700.0).with_font(字体.clone()),
            TextColor(Color::WHITE),
            Transform::from_xyz(0.0, -半径 - 3000.0, 3000.0),
            南极标签,
        ));
    });
}
