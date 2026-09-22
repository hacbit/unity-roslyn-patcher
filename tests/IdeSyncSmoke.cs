using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Xml.Linq;
using UnityEditor;
using UnityEngine;

public static class IdeSyncSmoke
{
    private static string expectedVersion;
    public static void Run() { Verify(); }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new Exception(message);
    }

    private static void CheckLanguage(string content)
    {
        var properties = XDocument.Parse(content).Descendants().Where(e => e.Name.LocalName == "LangVersion").ToArray();
        Require(properties.Length > 0 && properties.All(p => p.Value == expectedVersion), "Expected C# " + expectedVersion + " in generated project");
    }

    private static void Verify()
    {
        try
        {
            expectedVersion = System.Text.RegularExpressions.Regex.Match(
                File.ReadAllText(Environment.GetEnvironmentVariable("UNITY_ROSLYN_SESSION")),
                "\"lang_version\"\\s*:\\s*\"([0-9.]+)\"").Groups[1].Value;
            Require(!string.IsNullOrEmpty(expectedVersion), "Missing test language version");
            var root = Path.GetDirectoryName(Application.dataPath);
            var bridge = Type.GetType("UnityRoslynLauncher.ProjectSync, UnityRoslynLauncher.Editor", true);
            bridge.GetMethod("SyncExisting", BindingFlags.NonPublic | BindingFlags.Static).Invoke(null, null);
            var callback = bridge.GetMethod("OnGeneratedCSProject", BindingFlags.NonPublic | BindingFlags.Static);
            Func<string, string, string> rewrite = (path, content) => (string)callback.Invoke(null, new object[] { path, content });
            CheckLanguage(File.ReadAllText(Path.Combine(root, "GeneratedBeforeLaunch.csproj")));
            Require(File.ReadAllText(Path.Combine(root, "HandWritten.csproj")).Contains(">9.0<"), "Hand-written project was changed");
            Debug.Log("ROSLYN_IDE_EXISTING_OK");

            const string legacy = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<Project xmlns=\"http://schemas.microsoft.com/developer/msbuild/2003\">\r\n<!-- <LangVersion>ignored</LangVersion> -->\r\n<PropertyGroup><LangVersion>9.0</LangVersion><DefineConstants>A;B</DefineConstants></PropertyGroup>\r\n<PropertyGroup Condition=\"'$(Configuration)' == 'Debug'\"><LangVersion>10</LangVersion></PropertyGroup>\r\n</Project>";
            var projectPath = Path.Combine(root, "Fixture.csproj");
            var result = rewrite(projectPath, legacy);
            CheckLanguage(result);
            Require(result.Contains("<LangVersion>ignored</LangVersion>"), "XML comment was modified");
            Require(result.Contains("A;B") && result.Contains("utf-8") && result.Contains("\r\n"), "XML metadata or newline changed");
            Require(result == rewrite(projectPath, result), "Rewrite is not idempotent");
            CheckLanguage(rewrite(projectPath, "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>netstandard2.1</TargetFramework></PropertyGroup></Project>"));
            Require(rewrite(Path.Combine(root, "Assets/Other.csproj"), legacy) == legacy, "Out-of-scope project was changed");
            var session = Environment.GetEnvironmentVariable("UNITY_ROSLYN_SESSION");
            try {
                Environment.SetEnvironmentVariable("UNITY_ROSLYN_SESSION", null);
                Require(rewrite(projectPath, legacy) == legacy, "Bridge should be inactive outside launcher session");
            } finally { Environment.SetEnvironmentVariable("UNITY_ROSLYN_SESSION", session); }
            Debug.Log("ROSLYN_IDE_CALLBACK_OK");

            // Exercise the actual VS generation callback + file writer. Full Sync() needs
            // PackageManager metadata, unavailable in this isolated -noUpm fixture.
            var integration = Assembly.LoadFrom(Path.Combine(Application.dataPath, "Editor/Unity.VisualStudio.Editor.dll"));
            Debug.Log("IDE_GENERATOR_ASSEMBLY: " + integration.Location);
            var generatorType = integration.GetType("Microsoft.Unity.VisualStudio.Editor.ProjectGeneration", true);
            var generator = Activator.CreateInstance(generatorType, true);
            var sync = generatorType.GetMethod("SyncProjectFileIfNotChanged", BindingFlags.NonPublic | BindingFlags.Instance);
            for (var i = 0; i < 2; i++)
            {
                var legacyPath = Path.Combine(root, "ActualLegacy.csproj");
                var sdkPath = Path.Combine(root, "ActualSdk.csproj");
                sync.Invoke(generator, new object[] { legacyPath, legacy });
                sync.Invoke(generator, new object[] { sdkPath, "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><LangVersion>9.0</LangVersion></PropertyGroup></Project>" });
                CheckLanguage(File.ReadAllText(legacyPath));
                CheckLanguage(File.ReadAllText(sdkPath));
            }
            Debug.Log("ROSLYN_IDE_REGENERATION_OK");
            File.WriteAllText(Path.Combine(root, "ide-result.txt"), "PASS");
        }
        catch (Exception ex) { Debug.LogException(ex); throw; }
    }
}
