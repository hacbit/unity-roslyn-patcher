# 验证记录（2026-09-22）

环境：Windows x64；Unity 2022.3.40f1c1；.NET SDK 10.0.401；
Roslyn `5.9.0-1.26423.113`；最初测试使用 C# 12，后续覆盖测试见下文。

| 检查 | 结果 |
| --- | --- |
| Rust 参数解析、引号、注释、语言版本覆盖测试 | 通过 |
| 嵌套响应文件、UTF-16、循环引用拒绝 | 通过 |
| 子进程显式移除会话变量后的环境修复 | 通过 |
| 新编译器报错经代理、子进程、父进程返回失败码 | 通过 |
| 无注入的内置 Roslyn 编译 C# 12 样例 | 失败，符合负面对照预期 |
| 注入父进程 → 注入子进程 → 代理 → 新 Roslyn | 生成 Syntax.dll，退出码 0 |
| Unity Editor 首次脚本编译与代码执行 | `ROSLYN_EDITOR_OK` |
| Unity 内修改源文件并完成域重载 | `ROSLYN_HOT_RELOAD_OK`，读取到更新后的 Revision=2 |
| Windows x64 Mono Player 构建 | `ROSLYN_BUILD_OK` |
| Player 实际执行新版语法编译的代码 | `ROSLYN_PLAYER_OK: 42, 3`，退出码 0 |

调试版首次成功记录：`work/unity-cold.log`。
Release 版热重载记录：`work/unity-hot.log`；运行记录：`work/player.log`。
对应会话文件包含 Unity→Bee、Bee→cmd 的注入记录，以及 Editor/Player 程序集的代理命令。

最终发布版使用 `tests/run-unity-smoke.ps1 -DisablePackageManager` 全流程复验通过，
日志在 `work/smoke-20260922-112712-029/editor-test.log` 与 `player-test.log`。
`cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过。

真实链路比独立测试多了 `cmd.exe`，Bee 使用 `CreateProcessA`。
因此最终 Hook 同时覆盖 ANSI 与 Unicode 入口，并向目标 shell 传播。

最小项目 UPM 初始化时报 `The "path" argument must be of type string. Received undefined`。
未注入 Unity 的对照运行同样失败（`work/unity-baseline.log`），所以以上 Unity 验证加了
`-noUpm`。这证明脚本编译与构建链路可行，不代表已经验证业务项目的包兼容性。

所有实测均发生在本工作区的隔离测试项目，没有修改业务项目或 Unity 安装目录。

## IDE 项目语言版本同步

新增默认开启的 Editor 生成桥接脚本，嵌入启动器并自动部署到项目
`Assets/__UnityRoslynLauncher/Editor/`。验证时 `unity-launcher.example.json`
已配置为 C# 14；保留此用户配置，测试按会话配置读取预期值。

`tests/run-ide-smoke.ps1` 验收通过，日志：
`work/ide-smoke-20260922-120800-966/ide-test.log`。

- 实际 Unity 加载桥接程序集，将已有 Unity 生成文件从 9.0 更新为 14。
- 不修改无 Unity 生成标记的手写项目，也不处理项目根目录以外的路径。
- 验证 MSBuild XML 命名空间、条件 PropertyGroup、注释保持、缺失 LangVersion 补入、
  UTF-8 声明及 CRLF，以及重复处理不产生变化。
- 移除启动器会话环境变量后，回调返回原内容。
- 显式加载本机 VS 集成 DLL，调用其真实的 `SyncProjectFileIfNotChanged`：
  两次输入旧语言版本的 Legacy/SDK 项目内容，落盘结果都保持配置值 14。
- Rust 测试与 Clippy 检查通过。

本机最小项目仍使用 `-noUpm`。VS 插件的完整 `Sync()` 需要 UPM 包元数据，
在此隔离环境中不可用；因此测试覆盖真实生成回调及写入阶段，不声称验证了完整 UPM
初始化或 IDE 界面。Rider 的源码调用相同的 `OnGeneratedCSProject` 回调，尚未做 Rider 内实测。

## 默认版本与程序集响应文件覆盖

`tests/run-overrides-smoke.ps1` 通过，最终标记 `ROSLYN_OVERRIDES_HOT_AND_PLAYER_OK`。
日志：`work/overrides-20260922-141215-281/overrides.log`。
测试复制配置并仅在副本中设置默认 12，未改变用户示例配置中的 14 或发布配置中的 12。

- 无覆盖的 DefaultFeature 程序集使用 C# 12，主构造函数和集合表达式正常执行。
- High 使用相邻 `csc.rsp` 指定 14，`field` 属性语法编译并执行；define 参数保留。
- Low 显式指定 9.0，编译代理保留此覆盖，IDE 也为 9.0。
- Nested 通过 `csc.rsp` 引用另一个响应文件指定 14 和 define，实际编译及 IDE 同步通过。
- 各程序集的 Editor 与 `.Player.csproj` 回调结果均与其有效语言版本一致。
- 运行中添加 Hot 的响应文件并切换为 C# 14 源码，随后删除响应文件并恢复默认版本源码，
  均完成重新编译和域重载；已有项目文件自动同步，新生成回调也分别返回 14、12。
- 在同一会话完成 Windows Mono Player 构建。
- Rust 单元测试覆盖默认、显式高/低版本、重复参数取最后值、preview、嵌套文件和普通自定义响应文件；
  `cargo test` 与 Clippy 通过。

同时添加响应文件和修改源码的测试中，Unity 刷新期间先产生一次缺少 HOT_OPTION 的中间编译错误，
随后自行使用新响应文件重新编译成功；没有重启 Editor。上述验收指最终编译及生成结果。
测试仍使用 `-noUpm`，未新增 IL2CPP 或 IDE 界面内验证。

发布运行文件改为 `runtime/<Hook哈希>-<代理哈希>/`，由 `runtime.json` 指向；
旧 Unity 持有旧 DLL 时仍可发布新运行文件。新会话使用新版，已有会话需重新启动才会升级。
