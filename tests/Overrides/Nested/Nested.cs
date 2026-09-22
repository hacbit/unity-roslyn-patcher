#if !NESTED_OPTION
#error Nested response define must be preserved
#endif
public class Nested { public int Value { get; set => field = value; } }
