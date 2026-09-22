#if !HIGH_OPTION
#error csc.rsp define must be preserved
#endif
public class High
{
    // C# 14 semi-auto property, requiring the response override over default 12.
    public int Value { get; set => field = value; }
}
