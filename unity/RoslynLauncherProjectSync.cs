// UnityRoslynLauncher managed editor bridge. Updated by unity-launcher; do not edit this copy.
#if UNITY_EDITOR
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Xml;
using System.Xml.Linq;
using UnityEditor;
using UnityEditor.Compilation;
using UnityEngine;

namespace UnityRoslynLauncher
{
    // An independent Editor asmdef lets this load even when user scripts fail to compile.
    internal sealed class ProjectSync : AssetPostprocessor
    {
        [Serializable] private sealed class CompilerConfig
        {
            public string lang_version;
            public bool sync_ide;
        }
        [Serializable] private sealed class LaunchSession
        {
            public string project;
            public CompilerConfig config;
        }

        private static string ProjectRoot => Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        private static Dictionary<string, UnityEditor.Compilation.Assembly> editorOptions;
        private static Dictionary<string, UnityEditor.Compilation.Assembly> playerOptions;

        private static void InvalidateOptions() { editorOptions = null; playerOptions = null; }

        private static bool OnPreGeneratingCSProjectFiles()
        {
            InvalidateOptions();
            return false;
        }

        private static string AssemblyLanguage(string path, XElement root, string fallback)
        {
            var fileName = Path.GetFileNameWithoutExtension(path);
            var name = root.Elements(root.Name.Namespace + "PropertyGroup")
                .Elements(root.Name.Namespace + "AssemblyName").Select(e => e.Value).FirstOrDefault() ?? fileName;
            var player = fileName.EndsWith(".Player", StringComparison.OrdinalIgnoreCase);
            var options = player ? playerOptions : editorOptions;
            if (options == null)
            {
                options = CompilationPipeline.GetAssemblies(player ? AssembliesType.Player : AssembliesType.Editor)
                    .ToDictionary(a => a.name, a => a, StringComparer.Ordinal);
                if (player) playerOptions = options; else editorOptions = options;
            }
            UnityEditor.Compilation.Assembly assembly;
            if (!options.TryGetValue(name, out assembly))
            {
                if (!player || !name.EndsWith(".Player", StringComparison.OrdinalIgnoreCase) ||
                    !options.TryGetValue(name.Substring(0, name.Length - ".Player".Length), out assembly)) return fallback;
            }
            if (UsesUnityCompiler(assembly)) return "9.0";
            var compilerOptions = assembly.compilerOptions;
            var directories = CompilationPipeline.GetSystemAssemblyDirectories(compilerOptions.ApiCompatibilityLevel);
            // Do not use assembly.LanguageVersion: that is Unity's generated built-in default.
            // Bee writes custom compiler arguments first, followed by response file arguments.
            var version = ApplyLanguageArguments(compilerOptions.AdditionalCompilerArguments, fallback, directories, 0);
            foreach (var response in compilerOptions.ResponseFiles ?? new string[0])
                version = ApplyLanguageArguments(new[] { "@" + response }, version, directories, 0);
            return version;
        }

        private static bool UsesUnityCompiler(UnityEditor.Compilation.Assembly assembly)
        {
            string packageRoot = null;
            var sources = new List<string>();
            foreach (var source in assembly.sourceFiles ?? new string[0])
            {
                var full = Path.GetFullPath(Path.IsPathRooted(source) ? source : Path.Combine(ProjectRoot, source));
                if (!full.StartsWith(ProjectRoot.TrimEnd('\\', '/') + Path.DirectorySeparatorChar,
                        StringComparison.OrdinalIgnoreCase)) return false;
                var relative = full.Substring(ProjectRoot.TrimEnd('\\', '/').Length + 1)
                    .Replace('\\', '/').Split('/');
                string root = null;
                if (relative.Length >= 4 && relative[0].Equals("Library", StringComparison.OrdinalIgnoreCase) &&
                    relative[1].Equals("PackageCache", StringComparison.OrdinalIgnoreCase))
                    root = Path.Combine(ProjectRoot, "Library", "PackageCache", relative[2]);
                else if (relative.Length >= 3 && relative[0].Equals("Packages", StringComparison.OrdinalIgnoreCase))
                    root = Path.Combine(ProjectRoot, "Packages", relative[1]);
                else if (relative.Length >= 2 && relative[0].Equals("Library", StringComparison.OrdinalIgnoreCase) &&
                         relative[1].Equals("Bee", StringComparison.OrdinalIgnoreCase)) continue;
                else return false;
                if (packageRoot != null && !packageRoot.Equals(root, StringComparison.OrdinalIgnoreCase)) return false;
                packageRoot = root;
                sources.Add(full);
            }
            if (packageRoot == null) return false;
            foreach (var source in sources)
            {
                for (var directory = Path.GetDirectoryName(source); directory != null;
                     directory = Path.GetDirectoryName(directory))
                {
                    if (File.Exists(Path.Combine(directory, "csc.rsp"))) return false;
                    if (directory.Equals(packageRoot, StringComparison.OrdinalIgnoreCase)) break;
                }
            }
            return true;
        }

        private static string ApplyLanguageArguments(IEnumerable<string> arguments, string version, string[] directories, int depth)
        {
            if (depth > 16) throw new InvalidDataException("Cyclic or deeply nested response file");
            foreach (var raw in arguments ?? new string[0])
            {
                var argument = raw.Trim();
                if (argument.StartsWith("@", StringComparison.Ordinal))
                {
                    var file = argument.Substring(1).Trim('"');
                    var parsed = CompilationPipeline.ParseResponseFile(file, ProjectRoot, directories);
                    if (parsed.Errors != null && parsed.Errors.Length > 0)
                        throw new InvalidDataException(string.Join("; ", parsed.Errors));
                    version = ApplyLanguageArguments(parsed.OtherArguments, version, directories, depth + 1);
                }
                else if (argument.StartsWith("-langversion:", StringComparison.OrdinalIgnoreCase) ||
                         argument.StartsWith("/langversion:", StringComparison.OrdinalIgnoreCase))
                    version = argument.Substring(argument.IndexOf(':') + 1).Trim().Trim('"');
            }
            return version;
        }

        private static string LanguageVersion()
        {
            var sessionPath = Environment.GetEnvironmentVariable("UNITY_ROSLYN_SESSION");
            if (string.IsNullOrEmpty(sessionPath)) return null;
            var session = JsonUtility.FromJson<LaunchSession>(File.ReadAllText(sessionPath));
            if (session == null || session.config == null || !session.config.sync_ide ||
                string.IsNullOrEmpty(session.project) ||
                !string.Equals(Path.GetFullPath(session.project).TrimEnd('\\', '/'),
                    ProjectRoot.TrimEnd('\\', '/'), StringComparison.OrdinalIgnoreCase)) return null;
            var version = session.config.lang_version;
            if (string.IsNullOrEmpty(version) || !version.All(c => c >= '0' && c <= '9' || c == '.'))
                throw new InvalidDataException("Invalid launcher language version");
            return version;
        }

        private static bool IsProjectFile(string path)
        {
            var full = Path.GetFullPath(Path.IsPathRooted(path) ? path : Path.Combine(ProjectRoot, path));
            return string.Equals(Path.GetExtension(full), ".csproj", StringComparison.OrdinalIgnoreCase) &&
                string.Equals(Path.GetDirectoryName(full), ProjectRoot, StringComparison.OrdinalIgnoreCase);
        }

        // Called by Unity's Visual Studio / Rider project generators before writing the file.
        private static string OnGeneratedCSProject(string path, string content)
        {
            try
            {
                var version = LanguageVersion();
                return version != null && IsProjectFile(path) ? Rewrite(content, version, path) : content;
            }
            catch (Exception ex)
            {
                Debug.LogError("[RoslynLauncher] Cannot sync IDE language version: " + ex.Message);
                return content;
            }
        }

        private static string Rewrite(string content, string version, string path)
        {
            var settings = new XmlReaderSettings { DtdProcessing = DtdProcessing.Prohibit, XmlResolver = null };
            XDocument document;
            using (var input = new StringReader(content.TrimStart('\uFEFF')))
            using (var reader = XmlReader.Create(input, settings))
                document = XDocument.Load(reader, LoadOptions.PreserveWhitespace);
            var root = document.Root;
            if (root == null || root.Name.LocalName != "Project") return content;
            version = AssemblyLanguage(path, root, version);
            // Only actual MSBuild properties, not comments, task parameters or similarly named nodes.
            var properties = root.Elements(root.Name.Namespace + "PropertyGroup")
                .SelectMany(g => g.Elements(root.Name.Namespace + "LangVersion")).ToArray();
            if (properties.Length > 0 && properties.All(p => p.Value == version)) return content;
            if (properties.Length == 0)
                root.Add(new XElement(root.Name.Namespace + "PropertyGroup",
                    new XElement(root.Name.Namespace + "LangVersion", version)));
            else
                foreach (var property in properties) property.Value = version;
            var output = new StringBuilder();
            var writerSettings = new XmlWriterSettings {
                OmitXmlDeclaration = document.Declaration == null,
                NewLineHandling = NewLineHandling.Replace,
                NewLineChars = content.Contains("\r\n") ? "\r\n" : "\n",
                Indent = false
            };
            // StringWriter's default UTF-16 declaration would mislabel UTF-8 output files.
            using (var text = new Utf8Writer(output))
            using (var writer = XmlWriter.Create(text, writerSettings)) document.Save(writer);
            return output.ToString();
        }

        private sealed class Utf8Writer : StringWriter
        {
            internal Utf8Writer(StringBuilder buffer) : base(buffer) { }
            public override Encoding Encoding => new UTF8Encoding(false);
        }

        [InitializeOnLoadMethod]
        private static void OnLoad()
        {
            // Run again after domain reload; unchanged files are not rewritten.
            EditorApplication.delayCall += SyncExisting;
            CompilationPipeline.compilationFinished += _ => EditorApplication.delayCall += SyncExisting;
        }

        private static void OnPostprocessAllAssets(string[] imported, string[] deleted, string[] moved, string[] movedFrom)
        {
            if (imported.Concat(deleted).Concat(moved).Concat(movedFrom).Any(p =>
                p.EndsWith(".rsp", StringComparison.OrdinalIgnoreCase) || p.EndsWith(".asmdef", StringComparison.OrdinalIgnoreCase)))
            {
                InvalidateOptions();
                EditorApplication.delayCall += SyncExisting;
            }
        }

        private static void SyncExisting()
        {
            try
            {
                var version = LanguageVersion();
                if (version == null) return;
                InvalidateOptions();
                var updated = 0;
                foreach (var path in Directory.GetFiles(ProjectRoot, "*.csproj", SearchOption.TopDirectoryOnly))
                {
                    try
                    {
                        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0) continue;
                        // Existing files may include hand-authored tools. Only touch recognizably
                        // Unity-generated projects; callback output above is known to be generated.
                        var bytes = File.ReadAllBytes(path);
                        if (bytes.Length >= 2 && (bytes[0] == 0xFF || bytes[0] == 0xFE)) continue;
                        var content = new UTF8Encoding(false, true).GetString(bytes).TrimStart('\uFEFF');
                        if (!content.Contains("AssetPostprocessor.OnGeneratedCSProject") &&
                            !content.Contains("<UnityProjectGenerator>")) continue;
                        var replacement = Rewrite(content, version, path);
                        if (replacement == content) continue;
                        var bom = bytes.Length >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF;
                        File.WriteAllText(path, replacement, new UTF8Encoding(bom));
                        updated++;
                    }
                    catch (Exception ex) { Debug.LogError("[RoslynLauncher] Cannot update " + path + ": " + ex.Message); }
                }
                if (updated > 0) Debug.Log("[RoslynLauncher] Updated " + updated + " IDE projects (default C# " + version + ", assembly response overrides preserved)");
            }
            catch (Exception ex) { Debug.LogError("[RoslynLauncher] IDE sync failed: " + ex.Message); }
        }
    }
}
#endif
