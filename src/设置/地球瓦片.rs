use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};

// 瓦片浮在基准球面（经纬线圈所在半径）之外的微小偏移量，避免与线圈产生深度冲突。
// 取约 3.2 米：要大于 10000 量级下约 0.001 单位的浮点量化抖动，
// 同时低于相机的最低离地高度（10 米），相机不会钻到瓦片下方。
const 瓦片离球面高度: f32 = 0.005;
// 地球的世界单位半径，和经纬度线圈及各模块的半径常量保持一致。
const 地球半径: f32 = 10000.0;

// 这个组件标记当前场景中的瓦片根节点，所有瓦片实体都挂在它下面。
#[derive(Component)]
pub struct 地球瓦片根;

// 这个组件标记一个单独的瓦片实体，方便后面做可见区域更新和销毁。
#[derive(Component)]
pub struct 瓦片实体;

// 这个结构体记录某个瓦片的 zoom/x/y 编号，和天地图的名称规则完全一致。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Component)]
pub struct 瓦片编号 {
    pub zoom: u32,
    pub x: u32,
    pub y: u32,
}

// 这个结构体保存一个瓦片的地理范围，供后续计算球面上的位置与尺寸。
#[derive(Clone, Debug)]
pub struct 瓦片信息 {
    pub 编号: 瓦片编号,
    pub 东经最小: f64,
    pub 东经最大: f64,
    pub 北纬最大: f64,
    pub 南纬最小: f64,
}

// 这个资源缓存已加载的瓦片图片句柄，避免同一张瓦片重复从磁盘读取和解码。
#[derive(Resource, Default)]
pub struct 瓦片缓存 {
    pub 图片句柄: HashMap<瓦片编号, Handle<Image>>,
}

// 这个函数返回当前工程根目录下的地图瓦片目录，适配当前工作路径和 Bevy 运行路径。
pub fn 当前_地图目录() -> PathBuf {
    let 当前目录 = std::env::current_dir().expect("读取当前工程目录失败");
    当前目录.join("地图").join("石源村_2级").join("瓦片")
}

// 这个函数用来扫描目录下所有符合 zoom_x_y.png 规则的图片文件，并返回结果列表。
pub fn 扫描_瓦片目录(目录: &Path) -> Vec<PathBuf> {
    let Ok(读取结果) = std::fs::read_dir(目录) else {
        return Vec::new();
    };

    let mut 输出 = Vec::new();
    for 条目 in 读取结果.flatten() {
        let 路径 = 条目.path();
        if 路径.extension().and_then(|值| 值.to_str()) != Some("png") {
            continue;
        }
        let 名称 = 路径.file_stem().and_then(|值| 值.to_str());
        if 名称.is_none() {
            continue;
        }
        let 名称 = 名称.unwrap();
        let 片段: Vec<_> = 名称.split('_').collect();
        if 片段.len() != 3 {
            continue;
        }
        if 片段[0].parse::<u32>().is_ok() && 片段[1].parse::<u32>().is_ok() && 片段[2].parse::<u32>().is_ok() {
            输出.push(路径);
        }
    }

    输出.sort();
    输出
}

// 这个函数负责把文件名解析成 zoom/x/y 三个值，便于后续生成瓦片编号。
pub fn 解析_瓦片编号(路径: &Path) -> Option<瓦片编号> {
    let 名称 = 路径.file_stem()?.to_str()?;
    let 片段: Vec<_> = 名称.split('_').collect();
    if 片段.len() != 3 {
        return None;
    }

    let zoom = 片段[0].parse::<u32>().ok()?;
    let x = 片段[1].parse::<u32>().ok()?;
    let y = 片段[2].parse::<u32>().ok()?;

    Some(瓦片编号 { zoom, x, y })
}

// 这个函数返回 Web 墨卡托投影中某一行瓦片上边缘对应的纬度（角度制）。
// 天地图、OSM 等主流瓦片服务都使用这个公式，纬度不是均匀划分的。
pub fn 墨卡托纬度(行号: f64, 瓦片总数: f64) -> f64 {
    (PI * (1.0 - 2.0 * 行号 / 瓦片总数)).sinh().atan().to_degrees()
}

// 这个函数根据瓦片编号计算出每个瓦片在地球表面对应的经纬度范围，供后续放置到球面上。
pub fn 计算_瓦片_地理范围(编号: 瓦片编号) -> 瓦片信息 {
    let 瓦片总数 = (1u32 << 编号.zoom) as f64;
    let 经度步长 = 360.0 / 瓦片总数;

    瓦片信息 {
        编号,
        东经最小: 编号.x as f64 * 经度步长 - 180.0,
        东经最大: (编号.x as f64 + 1.0) * 经度步长 - 180.0,
        北纬最大: 墨卡托纬度(编号.y as f64, 瓦片总数),
        南纬最小: 墨卡托纬度(编号.y as f64 + 1.0, 瓦片总数),
    }
}

// 这个函数把经度和纬度转换成球面上的 3D 坐标。
// 经度约定与 地球信息::球面经纬度 保持一致：经度 = atan2(-z, x)，即东经方向朝 -Z。
pub fn 经纬度转球面坐标(经度度: f32, 纬度度: f32, 半径: f32) -> Vec3 {
    let (经度正弦, 经度余弦) = 经度度.to_radians().sin_cos();
    let (纬度正弦, 纬度余弦) = 纬度度.to_radians().sin_cos();
    Vec3::new(
        半径 * 纬度余弦 * 经度余弦,
        半径 * 纬度正弦,
        -半径 * 纬度余弦 * 经度正弦,
    )
}

// 这个函数返回某经纬度处球面上的“东向、北向、外向”三个正交单位向量，用于给瓦片定向。
pub fn 球面基向量(经度度: f32, 纬度度: f32) -> (Vec3, Vec3, Vec3) {
    let (经度正弦, 经度余弦) = 经度度.to_radians().sin_cos();
    let (纬度正弦, 纬度余弦) = 纬度度.to_radians().sin_cos();
    let 外向 = Vec3::new(纬度余弦 * 经度余弦, 纬度正弦, -纬度余弦 * 经度正弦);
    let 东向 = Vec3::new(-经度正弦, 0.0, -经度余弦);
    let 北向 = Vec3::new(-纬度正弦 * 经度余弦, 纬度余弦, 纬度正弦 * 经度正弦);
    (东向, 北向, 外向)
}

// 这个函数返回某个缩放级别下瓦片网格在经度方向需要切成的段数。
// 级别越低瓦片覆盖的角度越大，不细分的平面网格越容易整体凹陷到基准球面以内。
// 这里要求每一小段的弦向内凹陷不超过 瓦片离球面高度 的一半，保证细分后的网格始终浮在基准球面之外。
// 段数只依赖缩放级别，保证同一级别的所有瓦片用相同段数，相邻瓦片的共享边顶点数量一致。
pub fn 细分段数(zoom: u32) -> u32 {
    let 经度跨度弧度 = (360.0 / (1u64 << zoom) as f64).to_radians();
    // 弦凹陷约等于 半径 * 弧角² / 8，反解出允许的最大弧角。
    let 容许弧角 = (4.0 * 瓦片离球面高度 as f64 / 地球半径 as f64).sqrt();
    let 段数 = (经度跨度弧度 / 容许弧角).ceil() as u32;
    // 下限 1 表示不再细分；上限防止极低级别瓦片产生过多顶点。
    段数.clamp(1, 512)
}

// 这个函数返回瓦片网格第 i 列顶点对应的经度（角度制）。
// 东西边界直接取瓦片边界值，保证相邻瓦片在共享边上用到完全相同的经度；
// 中间列按比例插值，只影响瓦片内部、不与邻居共享。
fn 列经度(范围: &瓦片信息, i: u32, 段数: u32) -> f64 {
    if i == 0 {
        范围.东经最小
    } else if i == 段数 {
        范围.东经最大
    } else {
        let 比例 = i as f64 / 段数 as f64;
        范围.东经最小 + (范围.东经最大 - 范围.东经最小) * 比例
    }
}

// 这个函数返回瓦片网格第 j 行顶点对应的纬度（角度制）。
// 南北边界直接取瓦片边界值，保证相邻瓦片在共享边上用到完全相同的纬度；
// 中间行按比例插值，只影响瓦片内部、不与邻居共享。
fn 行纬度(范围: &瓦片信息, j: u32, 段数: u32) -> f64 {
    if j == 0 {
        范围.北纬最大
    } else if j == 段数 {
        范围.南纬最小
    } else {
        let 比例 = j as f64 / 段数 as f64;
        范围.北纬最大 + (范围.南纬最小 - 范围.北纬最大) * 比例
    }
}

// 这个函数把瓦片细分成 (段数+1)×(段数+1) 的顶点网格，并把每个顶点投射到球面上，
// 让网格贴合球面曲率，而不是只用四个角点拼出一块会凹陷的平面。
// 相邻瓦片共享的边界顶点由同一套公式和同一组经纬度算出，顶点坐标比特一致，
// 因此拼接处不会因浮点舍入产生缝隙。
pub fn 创建_瓦片网格(范围: &瓦片信息) -> Mesh {
    let 半径 = 地球半径 + 瓦片离球面高度;
    let 段数 = 细分段数(范围.编号.zoom);
    let 每边顶点 = 段数 + 1;

    let 顶点总数 = (每边顶点 * 每边顶点) as usize;
    let mut 位置: Vec<[f32; 3]> = Vec::with_capacity(顶点总数);
    let mut 法线: Vec<[f32; 3]> = Vec::with_capacity(顶点总数);
    let mut 纹理坐标: Vec<[f32; 2]> = Vec::with_capacity(顶点总数);

    // 行 j=0 对应北边（v=0），列 i=0 对应西边（u=0），与图像方向一致。
    for j in 0..每边顶点 {
        let 纬度 = 行纬度(范围, j, 段数);
        let v = j as f32 / 段数 as f32;
        for i in 0..每边顶点 {
            let 经度 = 列经度(范围, i, 段数);
            let u = i as f32 / 段数 as f32;
            let 点 = 经纬度转球面坐标(经度 as f32, 纬度 as f32, 半径);
            位置.push(点.into());
            法线.push(点.normalize().into());
            纹理坐标.push([u, v]);
        }
    }

    // 每个网格单元拆成两个三角形，绕向与原先的四角网格保持一致，正面朝向球外。
    let mut 索引: Vec<u32> = Vec::with_capacity((段数 * 段数 * 2 * 3) as usize);
    for j in 0..段数 {
        for i in 0..段数 {
            let 西北 = j * 每边顶点 + i;
            let 东北 = 西北 + 1;
            let 西南 = 西北 + 每边顶点;
            let 东南 = 西南 + 1;
            索引.extend_from_slice(&[西北, 西南, 东北, 东北, 西南, 东南]);
        }
    }

    let mut 网格 = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::all(),
    );
    网格.insert_attribute(Mesh::ATTRIBUTE_POSITION, 位置);
    网格.insert_attribute(Mesh::ATTRIBUTE_NORMAL, 法线);
    网格.insert_attribute(Mesh::ATTRIBUTE_UV_0, 纹理坐标);
    网格.insert_indices(Indices::U32(索引));
    网格
}

// 这个函数用于读取某个瓦片文件，并把图片转成 Bevy 可用的 Image 资源句柄。
pub fn 读取_瓦片图片(路径: &Path, 图像仓库: &mut Assets<Image>) -> Option<Handle<Image>> {
    let 内容 = std::fs::read(路径).ok()?;
    let 图像 = Image::from_buffer(
        &内容,
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::default(),
        true, // PNG 是 sRGB 编码，必须按 sRGB 解码，否则画面整体过曝
        bevy::image::ImageSampler::Default,
        bevy::asset::RenderAssetUsages::default(),
    )
    .ok()?;
    Some(图像仓库.add(图像))
}

// 这个函数创建一个瓦片实体：网格顶点就是瓦片四角在球面上的坐标，
// 实体本身不再需要平移、旋转和缩放。
pub fn 生成_瓦片实体(
    命令: &mut Commands,
    网格仓库: &mut ResMut<Assets<Mesh>>,
    材质仓库: &mut ResMut<Assets<StandardMaterial>>,
    图像仓库: &mut ResMut<Assets<Image>>,
    文件路径: &Path,
    父实体: Entity,
) {
    let Some(编号) = 解析_瓦片编号(文件路径) else {
        return;
    };

    let Some(纹理句柄) = 读取_瓦片图片(文件路径, 图像仓库) else {
        return;
    };

    let 地理范围 = 计算_瓦片_地理范围(编号);
    let material = 材质仓库.add(StandardMaterial {
        base_color_texture: Some(纹理句柄),
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });

    命令.spawn((
        Mesh3d(网格仓库.add(创建_瓦片网格(&地理范围))),
        MeshMaterial3d(material),
        Transform::IDENTITY,
        ChildOf(父实体),
        瓦片实体,
        编号,
    ));
}

// 这个函数负责启动时把地图目录下的全部瓦片贴到地球表面，并挂到球体根节点下。
pub fn 初始化地球瓦片(
    mut 命令: Commands,
    mut 网格仓库: ResMut<Assets<Mesh>>,
    mut 材质仓库: ResMut<Assets<StandardMaterial>>,
    mut 图像仓库: ResMut<Assets<Image>>,
    球体根查询: Query<Entity, With<crate::球体控制::球体根>>,
) {
    let Ok(球体根实体) = 球体根查询.single() else {
        warn!("未找到球体根节点，跳过瓦片加载");
        return;
    };

    let 地图目录 = 当前_地图目录();
    let 所有瓦片 = 扫描_瓦片目录(&地图目录);
    if 所有瓦片.is_empty() {
        warn!("地图目录为空，未找到瓦片：{}", 地图目录.display());
        return;
    }

    // 瓦片根节点挂到球体根下，保证拖拽旋转地球时瓦片跟随一起旋转。
    let 根实体 = 命令
        .spawn((
            Transform::default(),
            Visibility::default(),
            ChildOf(球体根实体),
            地球瓦片根,
        ))
        .id();

    for 路径 in &所有瓦片 {
        生成_瓦片实体(&mut 命令, &mut 网格仓库, &mut 材质仓库, &mut 图像仓库, 路径, 根实体);
    }

    info!("已加载 {} 张地图瓦片：{}", 所有瓦片.len(), 地图目录.display());
}

// 这个函数用于在后续更新阶段重新筛选可见瓦片，并更多地采用“只刷新变化区域”的方式来控制性能。
pub fn 更新地球瓦片(
    mut 命令: Commands,
    根查询: Query<Entity, With<地球瓦片根>>,
    现有查询: Query<Entity, With<瓦片实体>>,
    mut 图像仓库: ResMut<Assets<Image>>,
    mut 网格仓库: ResMut<Assets<Mesh>>,
    mut 材质仓库: ResMut<Assets<StandardMaterial>>,
) {
    // 这里先不做大规模重建，而是保留一个最小分页更新的接口，后续可以由相机视野范围驱动扩充。
    let Ok(根实体) = 根查询.single() else {
        return;
    };

    let 目录 = 当前_地图目录();
    let 所有瓦片 = 扫描_瓦片目录(&目录);
    let 可见瓦片 = 所有瓦片.into_iter().take(9).collect::<Vec<_>>();

    for 实体 in &现有查询 {
        命令.entity(实体).despawn();
    }

    for 路径 in &可见瓦片 {
        生成_瓦片实体(&mut 命令, &mut 网格仓库, &mut 材质仓库, &mut 图像仓库, 路径, 根实体);
    }
}

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 测试_解析_瓦片编号() {
        let 路径 = Path::new("/tmp/16_53632_28194.png");
        let 编号 = 解析_瓦片编号(路径).unwrap();
        assert_eq!(编号.zoom, 16);
        assert_eq!(编号.x, 53632);
        assert_eq!(编号.y, 28194);
    }

    #[test]
    fn 测试_计算_瓦片_地理范围() {
        let 编号 = 瓦片编号 { zoom: 16, x: 53632, y: 28194 };
        let 范围 = 计算_瓦片_地理范围(编号);
        assert!(范围.东经最小 < 范围.东经最大);
        assert!(范围.北纬最大 > 范围.南纬最小);
        // 与 地图/石源村_16级/信息.toml 记录的实际范围一致（Web 墨卡托投影）。
        assert!((范围.东经最小 - 114.609375).abs() < 1e-4);
        assert!((范围.北纬最大 - 24.3571).abs() < 1e-3);
        assert!((范围.南纬最小 - 24.3520).abs() < 1e-3);
    }

    #[test]
    fn 测试_球面基向量_正交且右手() {
        let (东向, 北向, 外向) = 球面基向量(114.61, 24.36);
        assert!((东向.length() - 1.0).abs() < 1e-6);
        assert!((北向.length() - 1.0).abs() < 1e-6);
        assert!(东向.dot(北向).abs() < 1e-6);
        assert!(东向.dot(外向).abs() < 1e-6);
        assert!((东向.cross(北向) - 外向).length() < 1e-6);
    }

    #[test]
    fn 测试_相邻瓦片_共享角点一致() {
        let 半径 = 地球半径 + 瓦片离球面高度;
        let 左 = 计算_瓦片_地理范围(瓦片编号 { zoom: 18, x: 214534, y: 112906 });
        let 右 = 计算_瓦片_地理范围(瓦片编号 { zoom: 18, x: 214535, y: 112906 });
        let 下 = 计算_瓦片_地理范围(瓦片编号 { zoom: 18, x: 214534, y: 112907 });

        // 东西相邻：左瓦片的东北角必须和右瓦片的西北角完全重合。
        assert_eq!(
            经纬度转球面坐标(左.东经最大 as f32, 左.北纬最大 as f32, 半径),
            经纬度转球面坐标(右.东经最小 as f32, 右.北纬最大 as f32, 半径),
        );
        // 南北相邻：左瓦片的西南角必须和下瓦片的西北角完全重合。
        assert_eq!(
            经纬度转球面坐标(左.东经最小 as f32, 左.南纬最小 as f32, 半径),
            经纬度转球面坐标(下.东经最小 as f32, 下.北纬最大 as f32, 半径),
        );
    }

    // 从网格中取出顶点位置，便于逐点比较。
    fn 网格顶点(网格: &Mesh) -> Vec<[f32; 3]> {
        match 网格.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
            bevy::render::mesh::VertexAttributeValues::Float32x3(值) => 值.clone(),
            _ => panic!("顶点位置格式不是 Float32x3"),
        }
    }

    #[test]
    fn 测试_细分网格_相邻瓦片_整条共享边一致() {
        // 6 级瓦片覆盖范围大，会触发网格细分，此时共享边必须逐点一致而不只是角点。
        let 左 = 计算_瓦片_地理范围(瓦片编号 { zoom: 6, x: 52, y: 26 });
        let 右 = 计算_瓦片_地理范围(瓦片编号 { zoom: 6, x: 53, y: 26 });
        assert_eq!(细分段数(左.编号.zoom), 细分段数(右.编号.zoom));

        let 段数 = 细分段数(左.编号.zoom) as usize;
        let 每边顶点 = 段数 + 1;
        let 左顶点 = 网格顶点(&创建_瓦片网格(&左));
        let 右顶点 = 网格顶点(&创建_瓦片网格(&右));

        // 左瓦片的东边缘（最后一列）要和右瓦片的西边缘（第一列）逐点完全重合。
        for j in 0..每边顶点 {
            assert_eq!(
                左顶点[j * 每边顶点 + 段数],
                右顶点[j * 每边顶点],
                "东西共享边第 {j} 行不一致",
            );
        }
    }

    #[test]
    fn 测试_细分网格_顶点不陷入球面() {
        // 低级别瓦片的所有顶点都必须浮在基准球面之外，网格才不会凹陷到线圈以内。
        let 范围 = 计算_瓦片_地理范围(瓦片编号 { zoom: 6, x: 52, y: 26 });
        let 顶点 = 网格顶点(&创建_瓦片网格(&范围));
        assert!(细分段数(6) > 1, "6 级瓦片必须被细分");
        for 点 in 顶点 {
            let 半径 = Vec3::from(点).length();
            assert!(
                半径 >= 地球半径,
                "顶点陷入球面以内：半径 {半径} < 地球半径 {地球半径}",
            );
        }
    }
}
