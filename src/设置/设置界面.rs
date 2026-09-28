use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy::ui::{FocusPolicy, Interaction, RelativeCursorPosition};
use bevy::winit::{UpdateMode, WinitSettings};
use bevy::window::{
    MonitorSelection, PresentMode, PrimaryWindow, WindowMode, WindowResolution,
};
use std::time::Duration;

use crate::帧率显示::帧率文本;
use crate::球体控制::球体相机;

// 面板右下角开关按钮的尺寸与间距，面板要贴在按钮正上方，所以两处都要用到。
const 按钮边距: f32 = 20.0;
const 面板高度预留: f32 = 62.0;

// 可选的窗口分辨率档位（物理像素，不含系统 DPI 缩放）。
const 分辨率表: &[(u32, u32)] = &[(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];

// 面板展开状态，同时记录指针是否停在界面元素上，供地球拖拽让位使用。
#[derive(Resource, Default)]
pub struct 界面状态 {
    pub 面板展开: bool,
    pub 指针在界面上: bool,
}

// 每一项可调节的画面设置，按钮和它的数值文本都靠这个枚举互相对应。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum 设置项 {
    分辨率,
    窗口模式,
    抗锯齿,
    垂直同步,
    失焦限帧,
    帧率显示,
}

// 当前生效的画面设置。改动由 应用画面设置 系统统一下发到窗口、相机和引擎配置。
#[derive(Resource, Clone, PartialEq, Debug)]
pub struct 画面设置 {
    分辨率索引: usize,
    全屏: bool,
    msaa: Msaa,
    垂直同步: bool,
    // 失焦时是否沿用 Bevy 默认的 60Hz 限帧（省电）。关掉后失焦也保持满帧。
    失焦限帧: bool,
    显示帧率: bool,
}

impl Default for 画面设置 {
    fn default() -> Self {
        画面设置 {
            分辨率索引: 0,
            全屏: false,
            msaa: Msaa::Sample2,
            垂直同步: false,
            失焦限帧: true,
            显示帧率: true,
        }
    }
}

impl 画面设置 {
    fn 分辨率(&self) -> (u32, u32) {
        分辨率表[self.分辨率索引]
    }
}

// 设置.toml 的文件名，放在项目根目录（也就是进程的工作目录）下。
const 设置文件名: &str = "设置.toml";

// 落盘用的中间结构。刻意不复用 画面设置：Msaa 是引擎类型，
// 存成采样数这种朴素值才能保证配置文件跨版本可读。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct 设置文件 {
    分辨率索引: usize,
    全屏: bool,
    msaa采样: u32,
    垂直同步: bool,
    失焦限帧: bool,
    显示帧率: bool,
}

impl Default for 设置文件 {
    fn default() -> Self {
        let 缺省 = 画面设置::default();
        设置文件 {
            分辨率索引: 缺省.分辨率索引,
            全屏: 缺省.全屏,
            msaa采样: 缺省.msaa.samples(),
            垂直同步: 缺省.垂直同步,
            失焦限帧: 缺省.失焦限帧,
            显示帧率: 缺省.显示帧率,
        }
    }
}

impl From<&画面设置> for 设置文件 {
    fn from(设置: &画面设置) -> Self {
        设置文件 {
            分辨率索引: 设置.分辨率索引,
            全屏: 设置.全屏,
            msaa采样: 设置.msaa.samples(),
            垂直同步: 设置.垂直同步,
            失焦限帧: 设置.失焦限帧,
            显示帧率: 设置.显示帧率,
        }
    }
}

impl 设置文件 {
    // 把文件内容写回 画面设置。所有越界值都在这里收敛，避免手写配置直接 panic。
    fn 应用到(&self, 设置: &mut 画面设置) {
        设置.分辨率索引 = self.分辨率索引.min(分辨率表.len() - 1);
        设置.全屏 = self.全屏;
        设置.msaa = match self.msaa采样 {
            1 | 2 | 4 | 8 => Msaa::from_samples(self.msaa采样),
            其他 => {
                warn!("[设置] msaa采样={其他} 不受支持，回退到 2x");
                Msaa::Sample2
            }
        };
        设置.垂直同步 = self.垂直同步;
        设置.失焦限帧 = self.失焦限帧;
        设置.显示帧率 = self.显示帧率;
    }
}

// 启动时读取 设置.toml。文件缺失时用默认值并立刻写出一份，方便用户直接编辑。
pub fn 读取设置文件(mut 设置: ResMut<画面设置>) {
    let 文本 = match std::fs::read_to_string(设置文件名) {
        Ok(文本) => 文本,
        Err(_) => {
            info!("[设置] 未找到 {设置文件名}，使用默认设置并创建该文件");
            保存设置文件(&设置);
            return;
        }
    };
    match toml::from_str::<设置文件>(&文本) {
        Ok(文件) => 文件.应用到(&mut 设置),
        Err(错误) => error!("[设置] {设置文件名} 解析失败：{错误}，本次使用默认设置"),
    }
}

// 立刻落盘。先写临时文件再改名替换，避免进程中途被杀留下半截配置。
fn 保存设置文件(设置: &画面设置) {
    let 内容 = match toml::to_string_pretty(&设置文件::from(设置)) {
        Ok(内容) => 内容,
        Err(错误) => {
            error!("[设置] 序列化失败：{错误}");
            return;
        }
    };
    let 临时路径 = format!("{设置文件名}.tmp");
    if let Err(错误) = std::fs::write(&临时路径, 内容) {
        error!("[设置] 写入 {临时路径} 失败：{错误}");
        return;
    }
    if let Err(错误) = std::fs::rename(&临时路径, 设置文件名) {
        error!("[设置] 保存到 {设置文件名} 失败：{错误}");
    }
}

#[derive(Component)]
pub(crate) struct 开关按钮;

#[derive(Component)]
pub(crate) struct 开关文本;

#[derive(Component)]
pub(crate) struct 设置面板;

// 数值按钮：点击后循环切换到下一档。
#[derive(Component)]
pub(crate) struct 设置按钮(设置项);

// 按钮内显示当前数值的文本。
#[derive(Component)]
pub(crate) struct 设置值文本(设置项);

const 颜色_面板: Color = Color::srgba(0.05, 0.07, 0.11, 0.94);
const 颜色_按钮: Color = Color::srgb(0.17, 0.26, 0.41);
const 颜色_按钮按下: Color = Color::srgb(0.28, 0.44, 0.68);
const 颜色_标签: Color = Color::srgb(0.86, 0.89, 0.94);

// 启动时创建右下角开关与默认折叠的设置面板。
pub fn 初始化设置界面(mut 命令: Commands, mut 字体_仓库: ResMut<Assets<Font>>) {
    let 字体 = crate::地球信息::读取中文字体(&mut 字体_仓库);

    let 面板 = 命令
        .spawn((
            设置面板,
            FocusPolicy::Block,
            RelativeCursorPosition::default(),
            BackgroundColor(颜色_面板),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                right: Val::Px(按钮边距),
                bottom: Val::Px(按钮边距 + 面板高度预留),
                width: Val::Px(340.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(14.0)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                ..default()
            },
        ))
        .id();

    for (标签, 项) in [
        ("分辨率", 设置项::分辨率),
        ("窗口模式", 设置项::窗口模式),
        ("抗锯齿 MSAA", 设置项::抗锯齿),
        ("垂直同步", 设置项::垂直同步),
        ("失焦限帧", 设置项::失焦限帧),
        ("帧率显示", 设置项::帧率显示),
    ] {
        let 行 = 命令
            .spawn(Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: Val::Px(12.0),
                ..default()
            })
            .id();

        let 标签实体 = 命令
            .spawn((
                Text::new(标签.to_string()),
                TextFont::from_font_size(20.0).with_font(字体.clone()),
                TextColor(颜色_标签),
            ))
            .id();

        let 数值按钮 = 命令
            .spawn((
                设置按钮(项),
                Button,
                FocusPolicy::Block,
                BackgroundColor(颜色_按钮),
                Node {
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
            ))
            .id();

        let 数值文本实体 = 命令
            .spawn((
                设置值文本(项),
                Text::new(String::new()),
                TextFont::from_font_size(20.0).with_font(字体.clone()),
                TextColor::WHITE,
            ))
            .id();

        命令.entity(行).add_child(标签实体).add_child(数值按钮);
        命令.entity(数值按钮).add_child(数值文本实体);
        命令.entity(面板).add_child(行);
    }

    // 开关按钮最后生成，保证在 UI 层级里位于面板之上，避免被面板遮挡点击。
    let 开关 = 命令
        .spawn((
            开关按钮,
            Button,
            FocusPolicy::Block,
            RelativeCursorPosition::default(),
            BackgroundColor(颜色_按钮),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(按钮边距),
                bottom: Val::Px(按钮边距),
                padding: UiRect::axes(Val::Px(18.0), Val::Px(9.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
        ))
        .id();

    let 开关文本实体 = 命令
        .spawn((
            开关文本,
            Text::new("设置"),
            TextFont::from_font_size(22.0).with_font(字体),
            TextColor::WHITE,
        ))
        .id();

    命令.entity(开关).add_child(开关文本实体);
}

// 点击开关展开或折叠面板；点击数值按钮则把该项循环到下一档。
pub(crate) fn 处理设置点击(
    交互查询: Query<
        (&Interaction, &设置按钮),
        (Changed<Interaction>, With<Button>),
    >,
    开关查询: Query<
        &Interaction,
        (Changed<Interaction>, With<开关按钮>),
    >,
    mut 界面状态: ResMut<界面状态>,
    mut 设置: ResMut<画面设置>,
    mut 面板查询: Query<&mut Node, With<设置面板>>,
    mut 开关文本查询: Query<&mut Text, With<开关文本>>,
) {
    for 交互 in &开关查询 {
        if *交互 == Interaction::Pressed {
            界面状态.面板展开 = !界面状态.面板展开;
            if let Ok(mut 节点) = 面板查询.single_mut() {
                节点.display = if 界面状态.面板展开 {
                    Display::Flex
                } else {
                    Display::None
                };
            }
            for mut 文本 in &mut 开关文本查询 {
                文本.0 = if 界面状态.面板展开 { "关闭" } else { "设置" }.to_string();
            }
        }
    }

    for (交互, 按钮) in &交互查询 {
        if *交互 != Interaction::Pressed {
            continue;
        }
        切换到下一档(&mut 设置, 按钮.0);
    }
}

// 按钮悬停与按下时的高亮反馈，纯视觉，不影响设置本身。
pub fn 更新按钮高亮(
    mut 交互查询: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Button>)>,
) {
    for (交互, mut 背景) in &mut 交互查询 {
        *背景 = BackgroundColor(match *交互 {
            Interaction::Pressed | Interaction::Hovered => 颜色_按钮按下,
            Interaction::None => 颜色_按钮,
        });
    }
}

// 把指针是否停在界面元素上写回 界面状态，供 球体控制 判断要不要让位。
// 放在 First 阶段执行，读上一帧的 UI 命中结果，足够拖拽判定使用。
pub fn 更新指针在界面(mut 界面状态: ResMut<界面状态>, 光标查询: Query<&RelativeCursorPosition>) {
    界面状态.指针在界面上 = 光标查询.iter().any(|光标| 光标.cursor_over());
}

// 数值文本只在设置变化时重写，避免每帧触发布局重算。
pub(crate) fn 同步设置文本(
    设置: Res<画面设置>,
    mut 文本查询: Query<(&设置值文本, &mut Text)>,
) {
    if !设置.is_changed() {
        return;
    }
    for (项, mut 文本) in &mut 文本查询 {
        文本.0 = 数值文本(&设置, 项.0);
    }
}

// 把设置差量下发到窗口、相机和 winit 配置。
pub fn 应用画面设置(
    设置: Res<画面设置>,
    mut 窗口查询: Query<&mut Window, With<PrimaryWindow>>,
    mut 相机查询: Query<&mut Msaa, With<球体相机>>,
    mut winit: ResMut<WinitSettings>,
    mut 帧率查询: Query<&mut Node, With<帧率文本>>,
    mut 上次: Local<Option<画面设置>>,
) {
    if 上次.as_ref() == Some(&*设置) {
        return;
    }
    // 首帧只是把 读取设置文件 的结果下发到引擎，不算用户改动，不必回写文件。
    let 是首次下发 = 上次.is_none();
    *上次 = Some(设置.clone());

    if let Ok(mut 窗口) = 窗口查询.single_mut() {
        let (宽, 高) = 设置.分辨率();
        if 设置.全屏 {
            // 用无边框全屏而不是独占全屏：Windows 上独占模式要求显示器提供匹配的 video mode，
            // 很多驱动/缩放组合下拿不到，Bevy 会直接跳过这次切换（表现为点了没反应）。
            窗口.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
        } else {
            窗口.mode = WindowMode::Windowed;
            // 覆盖 DPI 缩放，让“分辨率”表示真实渲染像素，而不是被系统放大后的逻辑尺寸。
            窗口.resolution =
                WindowResolution::new(宽, 高).with_scale_factor_override(1.0);
        }
        窗口.present_mode = if 设置.垂直同步 {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        };
    }

    for mut msaa in &mut 相机查询 {
        *msaa = 设置.msaa;
    }

    winit.unfocused_mode = if 设置.失焦限帧 {
        UpdateMode::reactive_low_power(Duration::from_secs_f64(1.0 / 60.0))
    } else {
        UpdateMode::Continuous
    };

    for mut 节点 in &mut 帧率查询 {
        // 用 Display::None 而不是 Hidden：连布局都省掉，隐藏时不再消耗 CPU。
        节点.display = if 设置.显示帧率 {
            Display::Flex
        } else {
            Display::None
        };
    }

    if !是首次下发 {
        保存设置文件(&设置);
    }
}

// 单项循环：每按一次切到下一档，最后一档回到第一档。
fn 切换到下一档(设置: &mut 画面设置, 项: 设置项) {
    match 项 {
        设置项::分辨率 => {
            设置.分辨率索引 = (设置.分辨率索引 + 1) % 分辨率表.len();
        }
        设置项::窗口模式 => 设置.全屏 = !设置.全屏,
        设置项::抗锯齿 => {
            设置.msaa = match 设置.msaa {
                Msaa::Off => Msaa::Sample2,
                Msaa::Sample2 => Msaa::Sample4,
                Msaa::Sample4 => Msaa::Sample8,
                Msaa::Sample8 => Msaa::Off,
            };
        }
        设置项::垂直同步 => 设置.垂直同步 = !设置.垂直同步,
        设置项::失焦限帧 => 设置.失焦限帧 = !设置.失焦限帧,
        设置项::帧率显示 => 设置.显示帧率 = !设置.显示帧率,
    }
}

// 生成某一项当前数值的显示文字。
fn 数值文本(设置: &画面设置, 项: 设置项) -> String {
    match 项 {
        设置项::分辨率 => {
            let (宽, 高) = 设置.分辨率();
            if 设置.全屏 {
                format!("{宽} x {高}（全屏无效）")
            } else {
                format!("{宽} x {高}")
            }
        }
        设置项::窗口模式 => (if 设置.全屏 { "全屏" } else { "窗口化" }).to_string(),
        设置项::抗锯齿 => match 设置.msaa {
            Msaa::Off => "关闭",
            Msaa::Sample2 => "2x",
            Msaa::Sample4 => "4x",
            Msaa::Sample8 => "8x",
        }
        .to_string(),
        设置项::垂直同步 => (if 设置.垂直同步 { "开启" } else { "关闭" }).to_string(),
        设置项::失焦限帧 => (if 设置.失焦限帧 { "限到 60" } else { "不限制" }).to_string(),
        设置项::帧率显示 => (if 设置.显示帧率 { "显示" } else { "隐藏" }).to_string(),
    }
}
