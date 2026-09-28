// 引入二维/三维向量类型，用于定位和观察方向。
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::math::Vec3;
// Bevy 0.19 中，3D 场景通常由组件组合而不是旧版 Bundle 构成。
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

// 引入帧率显示模块，专门负责窗口右上角 FPS 文本显示。
#[path = "设置/帧率显示.rs"]
mod 帧率显示;
// 引入地球标注模块，负责经纬度线圈与北极、南极文字标注。
#[path = "设置/地球标注.rs"]
mod 地球标注;
// 引入地球瓦片模块，负责把天地图瓦片贴到地球表面，并在可见范围内按需更新。
#[path = "瓦片/地球瓦片.rs"]
mod 地球瓦片;
// 引入地球信息模块，负责读取鼠标落点的经纬度、离地高度并在左上角显示。
#[path = "设置/地球信息.rs"]
mod 地球信息;
// 引入球体控制模块，负责拖拽旋转与滚轮缩放这两个核心交互逻辑。
#[path = "相机/球体控制.rs"]
mod 球体控制;
// 引入设置界面模块，负责右下角开关与画面/帧率设置面板。
#[path = "设置/设置界面.rs"]
mod 设置界面;

use 地球信息::{初始化地球信息, 更新地球信息, 地理状态};
use 地球标注::生成_地球标注;
use 地球瓦片::{初始化地球瓦片, 经纬度转球面坐标};
use 帧率显示::{初始化帧率显示, 更新帧率文本};
use 球体控制::控制_地球_视角;
use 设置界面::{
    初始化设置界面, 应用画面设置, 同步设置文本, 处理设置点击, 更新指针在界面, 更新按钮高亮, 读取设置文件,
    画面设置, 界面状态,
};

// 程序入口函数，启动整个 Bevy 应用。
fn main() {
    // 创建一个新的应用实例，负责管理世界、资源、插件和系统。
    App::new()
        // 设置清屏颜色，使用深蓝色夜空背景，增强地球的观感。
        .insert_resource(ClearColor(Color::srgb(0.02, 0.04, 0.09)))
        // 启用默认插件，并自定义窗口参数，这里保持默认渲染与输入功能。
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            // 只定义主窗口，确保当前窗口是应用的主要显示窗口。
            primary_window: Some(Window {
                // 设置窗口标题，展示为“地球模拟器 - Bevy”。
                title: "地球模拟器 - Bevy".to_string(),
                // 设置窗口分辨率，使用 1280x720。
                resolution: WindowResolution::new(1280, 720),
                // Bevy 会在支持“无垂直同步”时自动选择 Immediate/Mailbox，
                // 如果当前平台不支持，则会安全回退到普通 FIFO。
                // 这样比直接写 Immediate 更稳妥，能避免某些桌面环境仍然被 60Hz 卡住。
                present_mode: PresentMode::AutoNoVsync,
                // 继承默认的其他窗口设置。
                ..default()
            }),
            // 其余窗口设置保持默认。
            ..default()
        }))
        // 打开 Bevy 的帧率诊断插件，用于读取 FPS 数值。
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        // 初始化地理状态资源，供左上角的经纬度和高度显示系统读取。
        .init_resource::<地理状态>()
        // 初始化设置面板的展开状态与画面设置资源。
        .init_resource::<界面状态>()
        .init_resource::<画面设置>()
        // 在启动阶段按顺序执行：先创建地球、灯光、相机，再创建文本显示，
        // 最后加载地图瓦片（瓦片系统需要查询 初始化场景 创建的球体根节点）。
        .add_systems(
            Startup,
            (
                读取设置文件,
                初始化场景,
                初始化帧率显示,
                初始化地球信息,
                初始化地球瓦片,
                初始化设置界面,
            )
                .chain(),
        )
        // 添加最小交互逻辑：左键拖拽旋转球体，滚轮控制相机缩放。
        .add_systems(Update, 控制_地球_视角)
        .add_systems(Update, 更新帧率文本)
        .add_systems(Update, 更新地球信息)
        // 设置面板：先算指针命中，再处理点击、刷新文字与下发到窗口/相机。
        .add_systems(First, 更新指针在界面)
        .add_systems(
            Update,
            (处理设置点击, 更新按钮高亮, 同步设置文本, 应用画面设置).chain(),
        )
        // 运行应用主循环，直到窗口关闭。
        .run();
}

// 初始化场景函数，在启动时创建地球根节点、经纬度线圈标注和相机。
fn 初始化场景(
    // 命令对象，用于向世界中生成实体、组件和资源。
    mut 命令: Commands,
    // 网格资源仓库，用于创建和管理三维网格数据，如球体、立方体等。
    mut 网格_仓库: ResMut<Assets<Mesh>>,
    // 材质资源仓库，用于创建和管理 PBR 材质，例如地球表面颜色。
    mut 材质_仓库: ResMut<Assets<StandardMaterial>>,
    // 字体资源仓库，用于加载并绑定北/南极标注所需的真实字体文件。
    mut 字体_仓库: ResMut<Assets<Font>>,
) {
    // 先创建唯一的地球根节点，经纬度线圈、两极标注和瓦片都挂在它下面，保证旋转同步。
    // Bevy 0.19 中，根节点不再依赖 SpatialBundle，直接使用 Transform 组件即可。
    // 补充 Visibility：它会带上 InheritedVisibility/ViewVisibility，
    // 否则子实体（瓦片、文字标注）会触发 B0004 层级警告。
    let 地球根 = 命令
        .spawn((
            Transform::default(),
            Visibility::default(),
            球体控制::球体根,
        ))
        .id();

    // 在同一启动流程中把经纬度网格和极地文字挂接到同一个地球根节点上，避免出现“线圈不跟随旋转”的问题。
    // 这里不再绘制实体球：瓦片就是地球表面，线框风格下也不会有蓝色球体从凹陷瓦片处透出。
    生成_地球标注(&mut 命令, &mut 网格_仓库, &mut 材质_仓库, &mut 字体_仓库, 地球根);

    // 相机初始位置对准石源村瓦片区域中心（见 地图/石源村_16级/信息.toml），
    // 离地约 120 单位，启动后即可直接看到贴附在球面上的卫星瓦片。
    let 相机位置 = 经纬度转球面坐标(114.788, 24.174, 10000.0 + 120.0);
    命令.spawn((
        Camera3d::default(),
        // 近平面降到约 0.3 米，相机贴近地面时才不会把地面裁掉；
        // Bevy 0.19 使用无限远反向 Z 深度，极小近平面不影响远处精度。
        Projection::Perspective(PerspectiveProjection {
            near: 0.0005,
            ..default()
        }),
        Transform::from_translation(相机位置).looking_at(Vec3::ZERO, Vec3::Y),
        球体控制::球体相机,
    ));
}

