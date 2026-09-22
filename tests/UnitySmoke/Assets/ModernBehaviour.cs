using UnityEngine;

namespace LauncherSmoke;

public class ModernValue(int value)
{
    public const int Revision = 1;
    public int Value => value;
    public int[] Items => [1, 2, 3];
}

public class ModernBehaviour : MonoBehaviour
{
    private void Awake()
    {
        var value = new ModernValue(42);
        Debug.Log($"ROSLYN_PLAYER_OK: {value.Value}, {value.Items.Length}");
        Application.Quit(value.Value == 42 && value.Items.Length == 3 ? 0 : 1);
    }
}
