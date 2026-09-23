# Unity Roslyn Launcher

Windows x64 的轻量 Unity 启动器。Rust 实现，通过 FFI 使用微软 Detours 4.0.1。
不修改 Unity 安装目录；指定 Roslyn 只应用于本次启动的 Unity 编译进程树。

## 使用

先关闭目标项目的 Unity，再从启动器仓库目录执行：

```powershell
.\dist\unity-launcher.exe --project .\work\UnitySmoke
```

也可以将 `dist` 文件放到自选位置，再指定项目目录：

```powershell
.\unity-launcher.exe --project <Unity 项目目录>
```

一键启动：把仓库里的 `Open-Unity.example.cmd` 复制到 Unity 项目根目录，双击即可。
若移动工具安装目录，修改脚本里的 EXE 路径。工具目录需要 ASCII 路径；含空格路径受支持。

发布时请复制整个 `dist/`，保留 `unity-launcher.exe`、`unity-launcher.json`、
`runtime.json` 及其指向的 `runtime/<版本>/` 中的 Hook DLL 和编译代理。
运行文件按版本存放，已打开的 Unity 可以继续使用旧版本，新启动的项目使用新版本。
Roslyn 和 .NET 运行时使用配置指定的本地安装，不打包在内。
`chain-probe.exe` 只用于自测。

运行 `./package.ps1` 可先编译 Release，再将 `dist` 的发布文件原样压成
`dist/unity-roslyn-launcher.zip`。ZIP 顶层就是 `unity-launcher.exe`、配置、
`runtime.json` 和 `runtime/`，不预设 Unity 项目的目录布局。仅复用现有构建产物时
可传 `-SkipBuild`。已有旧版本运行目录、备份清单和旧 ZIP 不会进入新包。
包内 `.gitignore` 忽略解压后的 `runtime/` 和 `runtime.json`；ZIP 内仍必须包含它们。

推送 `v1.2.3` 形式的标签会触发 GitHub Actions，在 Windows 上构建并创建同名
Release，附件名为 `unity-roslyn-launcher-v1.2.3.zip`。发布包不包含 Unity 或 .NET SDK，
使用者需要先安装所需版本并修改 `unity-launcher.json`。普通提交不会触发发布。

配置按 `--config`、项目根目录 `unity-launcher.json`、EXE 同目录的顺序查找。
相对路径以配置文件所在目录为基准。

```json
{
  "dotnet": "<dotnet.exe 的绝对路径>",
  "csc": "<Roslyn/bincore/csc.dll 的绝对路径>",
  "lang_version": "12"
}
```

未填写 `unity` 时，根据项目 `ProjectSettings/ProjectVersion.txt` 自动查找：
`UNITY_EDITOR_ROOT` 环境变量指定的目录、启动器内置的编辑器目录和
Unity Hub 安装目录。也可添加 `"unity": "<Editor/Unity.exe 的绝对路径>"` 显式指定，
此时由使用者保证它与项目版本匹配。

启动前会实际运行指定编译器，检查运行时能否承载它，以及是否支持指定的语言版本。
请保留 Roslyn 完整目录中的依赖 DLL、deps.json、runtimeconfig.json。

### 默认版本与程序集覆盖

`lang_version` 是项目程序集的默认语言版本。未配置包内 `csc.rsp` 的第三方包
使用 Unity 自带编译器和 C# 9，不受根 `Assets/csc.rsp` 影响；包内显式配置
`csc.rsp` 后使用指定的新编译器。包内配置的 `-langversion` 优先，包括低于默认值的版本。
例如配置默认 `12`，在目标 `.asmdef` 同目录的 `csc.rsp` 中写入：

```text
-langversion:14
-define:MY_FEATURE
-nullable:enable
```

该程序集按 C# 14 编译，其 `.csproj` 生成 `<LangVersion>14</LangVersion>`；
其他项目程序集仍为 12。显式指定 `-langversion:9.0` 也会保留为 9.0。
响应文件的适用范围由 Unity 决定，不将某个程序集的配置扩散到其他程序集。
支持嵌套 `@rsp`；多个语言版本参数按最终编译参数顺序取最后一个，
不会取数值最大的版本。其他参数继续传给编译器。

修改、添加或删除 `csc.rsp` 后，等待 Unity 刷新和重新编译即可；删除语言版本覆盖后回到默认值。
JSON 配置在启动时快照保存，修改 JSON 默认值仍需重新通过启动器打开项目。

## IDE 语言版本同步

默认开启。启动器会在项目 `Assets/__UnityRoslynLauncher/Editor/` 自动部署一个
独立的 Editor-only asmdef 和项目生成回调脚本，源码嵌入启动器，无需手动复制。

- Unity 加载后，将项目根目录已有的、带 Unity 生成标记的 `.csproj` 的 `LangVersion`
  同步为本次启动配置的默认版本，并按各程序集的编译参数和响应文件应用覆盖，不会改手写工具项目。
- VS / Rider 集成插件调用 `OnGeneratedCSProject` 时，在落盘前同步语言版本，
  所以重新生成解决方案不会恢复成 9.0。兼容 MSBuild 命名空间和 SDK 风格项目。
- Editor 域重载、编译完成或响应文件/asmdef 变化后再次检查已有文件；内容一致时不重复写入。
- 回调只在匹配当前项目的启动器会话中生效。从 Hub 直接启动时脚本不干预生成。
- 脚本位于独立 Editor-only 程序集，不进入 Player。该目录和 Unity 生成的 `.meta`
  会留在项目中，可按团队习惯纳入或排除版本控制；不要手工修改自动生成的脚本。

需要关闭时，在所用 JSON 配置中增加 `"sync_ide": false`，再重启 Unity。
已经修改的 `.csproj` 会保留到下次重新生成；桥接目录不会自动删除。
升级本启动器后需要关闭项目并重新通过启动器打开，IDE 如未刷新可重载解决方案。

IDE 自身仍需支持所选 C# 版本；这个功能同步项目配置，不替换 IDE 的语言服务。

其他参数：

```powershell
# 只校验配置，不改项目、不启动 Unity
.\dist\unity-launcher.exe --project .\work\UnitySmoke --dry-run

# 强制归档编译缓存并重编译
.\dist\unity-launcher.exe --project .\work\UnitySmoke --rebuild

# -- 之后原样作为 Unity 参数；--wait 返回 Unity 的退出码
.\dist\unity-launcher.exe --project .\work\UnitySmoke --wait -- -batchmode -quit -logFile editor.log

# 独立两级进程注入测试，不启动 Unity；显式配置包含 Unity 路径
.\dist\unity-launcher.exe --config .\unity-launcher.example.json --self-test
```

## 实际链路

本机 Unity 2022.3.40f1c1 实测链路为：

```text
unity-launcher.exe
  Unity.exe [Hook]
    bee_backend.exe [Hook]
      cmd.exe [Hook，仅包含目标 csc 命令的 shell]
        compiler-proxy.exe
          指定 dotnet.exe exec 指定 csc.dll @处理后的.rsp
```

- Detours 在进程入口执行前加载 Hook DLL；同时拦截 `CreateProcessW` 和 `CreateProcessA`。
- 只对精确配置的 Bee 路径及目标编译器 shell 传播 Hook。
- 原始命令必须匹配当前 Unity 的 `NetCoreRuntime/dotnet.exe exec DotNetSdkRoslyn/csc.dll` 才替换。
- 显式子进程环境中补入本次会话配置，保留调用方的工作目录、句柄、标准输出管道和挂起标志。
- 编译代理展开 UTF-8 / 带 BOM 的 UTF-16 响应文件及嵌套 `@rsp`，移除 `/shared`。
  仅去掉识别出的 Bee 顶层生成响应文件中第一个内置语言版本参数，前置配置默认值，保留后续用户覆盖。
  当前识别路径为 `Library/Bee/artifacts/*.dag/*.rsp`；不会把任意用户响应文件的第一个参数删掉。
  其余源文件、引用、analyzer、define、输出参数继续使用 Unity 给出的值。
- 不把新 SDK 的引用程序集塞给 Unity；编译器运行在新 .NET 上，产物仍以 Unity 引用程序集为目标。
- 每次编译记录真实命令、处理后的响应文件及退出码，日志在
  `Library/RoslynLauncher/sessions/<会话>/`。

## 缓存与恢复

第一次接管项目，或编译器、配置、启动器、代理、Hook 发生变化时，会将
`Library/Bee` 和 `Library/ScriptAssemblies` 移入
`Library/RoslynLauncher/cache-backups/<会话>/`，再由 Unity 重建。
不会删除旧缓存，备份会占用磁盘空间；大型项目第一次启动会增加编译时间。

持续通过启动器打开同一项目，可以复用增量缓存。
若中途绕过启动器，使用 Unity Hub/原生 Unity 编译过项目，再回来时请加 `--rebuild`，
因为 Bee 不知道进程 Hook 改写了实际编译器。

恢复内置编译器：关闭 Unity，将 `Library/Bee` 和 `Library/ScriptAssemblies` 移出项目
（保留备份），再直接从 Unity Hub 打开，让内置编译器重新编译。
项目源代码若使用了超出内置编译器能力的语法，恢复后会正常报语法错误。

## 构建与测试

需要 Rust x64 MSVC 工具链、Visual Studio C++ Build Tools / Windows SDK。
业务逻辑均为 Rust，C++ 构建环境用于编译 vendored Detours。

```powershell
cargo test --workspace --locked
.\build.ps1
```

`build.ps1` 不覆盖已有的 `dist/unity-launcher.json`。
源码固定使用 `vendor/Detours-4.0.1`，来源：
https://github.com/microsoft/Detours/releases/tag/v4.0.1 ，许可证保留在原目录和发布目录。

隔离 Unity 测试脚本：

```powershell
.\tests\run-unity-smoke.ps1 -DisablePackageManager
```

它复制测试项目到工作区 `work/`，验证首次编译、运行中修改脚本和域重载、
Windows Mono Player 构建及实际运行。不会打开你的业务项目。
本机最小项目的 UPM 初始化在不带 Hook 时也报错，所以实测使用 `-noUpm`；
该参数仅是测试环境的绕过方式，日常启动器不会自动添加它。

IDE 生成回调验收：`tests/run-ide-smoke.ps1 -VisualStudioDll <本机Unity.VisualStudio.Editor.dll>`。
该测试在隔离 Unity 中验证已有文件同步、XML 处理及 VS 插件真实的回调/落盘路径；
因禁用了 UPM，不调用依赖包元数据的完整 `Sync()`。测试 DLL 只复制到忽略的 `work/` 中。

程序集覆盖验收：`tests/run-overrides-smoke.ps1`，在隔离项目中验证默认 12、覆盖 14/9.0、
嵌套响应文件、Editor/Player 项目 LangVersion、运行中添加/删除覆盖及 Windows Mono Player 构建。

## 已验证范围与限制

- 已验证：Windows x64，Unity 2022.3.40f1c1，SDK 10.0.401 自带 Roslyn，默认 C# 12、程序集覆盖 14/9.0。
- 特性样例：文件作用域命名空间、主构造函数、集合表达式、C# 14 的 `field` 属性。
- 尚未验证：其他 Unity 版本、业务项目的全部包与 Source Generator、IL2CPP、Android。
- 不升级 Unity Mono / IL2CPP / 基础类库；依赖新运行时的 C# 特性仍可能不可用。
- 已验证 VS 集成生成器实际的回调及写文件路径；Rider 使用相同生成回调，尚未做 Rider IDE 内部验证。
- 首版禁用编译服务器，编译性能可能低于 Unity 原来的 `/shared` 模式。
- 只支持启动新 Editor，不接管已经打开的 Unity。两次并发启动同一项目应避免。
- 不支持带扩展 STARTUPINFOEX 的目标 ANSI 启动调用；遇到时显式失败。
- 本方案属于外部编译链替换原型，不是 Unity 官方扩展点。

验证记录见仓库 `docs/verification.md`。
