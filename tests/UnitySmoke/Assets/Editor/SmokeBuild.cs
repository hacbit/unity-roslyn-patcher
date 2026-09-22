using System;
using UnityEditor;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;
using LauncherSmoke;

public static class SmokeBuild
{
    public static void HotReload()
    {
        var path = "Assets/ModernBehaviour.cs";
        var source = System.IO.File.ReadAllText(path);
        if (!source.Contains("Revision = 1")) throw new Exception("Hot reload fixture is not fresh");
        SessionState.SetBool("RoslynSmoke.Reload", true);
        System.IO.File.WriteAllText(path, source.Replace("Revision = 1", "Revision = 2"));
        AssetDatabase.Refresh();
    }

    [InitializeOnLoadMethod]
    private static void AfterReload()
    {
        if (!SessionState.GetBool("RoslynSmoke.Reload", false)) return;
        SessionState.EraseBool("RoslynSmoke.Reload");
        EditorApplication.delayCall += () => {
            try {
                if (ModernValue.Revision != 2) throw new Exception("Stale assembly after hot reload");
                Debug.Log("ROSLYN_HOT_RELOAD_OK");
                Run();
                EditorApplication.Exit(0);
            } catch (Exception e) { Debug.LogException(e); EditorApplication.Exit(1); }
        };
    }

    public static void Run()
    {
        var value = new ModernValue(42);
        if (value.Value != 42 || value.Items.Length != 3)
            throw new Exception("New syntax compiled but returned incorrect values");
        Debug.Log("ROSLYN_EDITOR_OK");
        var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
        new GameObject("ModernBehaviour").AddComponent<ModernBehaviour>();
        EditorSceneManager.SaveScene(scene, "Assets/Smoke.unity");
        PlayerSettings.SetScriptingBackend(BuildTargetGroup.Standalone, ScriptingImplementation.Mono2x);
        var report = BuildPipeline.BuildPlayer(new BuildPlayerOptions {
            scenes = new[] { "Assets/Smoke.unity" },
            locationPathName = "Build/Smoke.exe",
            target = BuildTarget.StandaloneWindows64,
            options = BuildOptions.Development
        });
        if (report.summary.result != BuildResult.Succeeded)
            throw new Exception("Player build failed: " + report.summary.result);
        Debug.Log("ROSLYN_BUILD_OK");
    }
}
