using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Xml.Linq;
using UnityEditor;
using UnityEngine;

public static class OverridesSmoke
{
    private const string State = "RoslynOverridesSmoke.Stage";
    private static void Require(bool value, string message) { if (!value) throw new Exception(message); }

    private static void Verify(string hotVersion)
    {
        var bridge = Type.GetType("UnityRoslynLauncher.ProjectSync, UnityRoslynLauncher.Editor", true);
        var flags = BindingFlags.NonPublic | BindingFlags.Static;
        bridge.GetMethod("OnPreGeneratingCSProjectFiles", flags).Invoke(null, null);
        var callback = bridge.GetMethod("OnGeneratedCSProject", flags);
        var root = Path.GetDirectoryName(Application.dataPath);
        foreach (var pair in new[] { new[] {"DefaultFeature", "12"}, new[] {"High", "14"}, new[] {"Low", "9.0"}, new[] {"Nested", "14"}, new[] {"Hot", hotVersion} })
        {
            foreach (var suffix in new[] { "", ".Player" })
            {
                var path = Path.Combine(root, pair[0] + suffix + ".csproj");
                var content = "<Project><!-- AssetPostprocessor.OnGeneratedCSProject --><PropertyGroup><AssemblyName>" + pair[0] + "</AssemblyName><LangVersion>9.0</LangVersion></PropertyGroup></Project>";
                var output = (string)callback.Invoke(null, new object[] { path, content });
                var version = XDocument.Parse(output).Descendants("LangVersion").Single().Value;
                Require(version == pair[1], "IDE mismatch for " + pair[0] + suffix + ": " + version + " != " + pair[1]);
                File.WriteAllText(path, output);
            }
        }
        var high = new High(); high.Value = 42;
        Require(high.Value == 42 && new DefaultFeature(12).Values.Length == 3 && Low.Read() == 9, "Runtime result mismatch");
        Debug.Log("ROSLYN_OVERRIDES_OK: default=12, High=14, Low=9.0, Hot=" + hotVersion);
    }

    public static void Run()
    {
        Verify("12");
        Require(Hot.Read() == 12, "Unexpected initial Hot assembly");
        SessionState.SetInt(State, 1);
        AssetDatabase.StartAssetEditing();
        try {
            File.WriteAllText("Assets/Overrides/Hot/csc.rsp", "-langversion:14\n/define:HOT_OPTION\n");
            File.WriteAllText("Assets/Overrides/Hot/Hot.cs", "#if !HOT_OPTION\n#error HOT_OPTION missing\n#endif\npublic class Hot { public int Value { get; set => field = value; } public static int Read() { var v = new Hot(); v.Value = 14; return v.Value; } }\n");
        } finally { AssetDatabase.StopAssetEditing(); }
        AssetDatabase.Refresh();
    }

    [InitializeOnLoadMethod]
    private static void Reload()
    {
        if (SessionState.GetInt(State, 0) == 0) return;
        EditorApplication.delayCall += () => {
            try {
                var stage = SessionState.GetInt(State, 0);
                if (stage == 1) {
                    Require(Hot.Read() == 14, "Hot csc.rsp edit did not recompile as C# 14");
                    Verify("14");
                    SessionState.SetInt(State, 2);
                    AssetDatabase.StartAssetEditing();
                    try {
                        File.Delete("Assets/Overrides/Hot/csc.rsp");
                        File.WriteAllText("Assets/Overrides/Hot/Hot.cs", "public class Hot { public static int Read() { return 12; } }\n");
                    } finally { AssetDatabase.StopAssetEditing(); }
                    AssetDatabase.Refresh();
                } else if (stage == 2) {
                    Require(Hot.Read() == 12, "Deleted override did not restore the default");
                    Verify("12");
                    SessionState.EraseInt(State);
                    SmokeBuild.Run();
                    Debug.Log("ROSLYN_OVERRIDES_HOT_AND_PLAYER_OK");
                    EditorApplication.Exit(0);
                }
            } catch (Exception e) { Debug.LogException(e); EditorApplication.Exit(1); }
        };
    }
}
