using System.Diagnostics;

namespace AsignacionDelCielo_WinUI.Services;

/// <summary>Append-only crash / diagnostic log under LocalAppData.</summary>
public static class CrashLog
{
    private static readonly object Gate = new();

    public static string LogPath
    {
        get
        {
            var dir = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "asignacion-del-cielo-bible");
            Directory.CreateDirectory(dir);
            return Path.Combine(dir, "winui-crash.log");
        }
    }

    public static void Write(string message, Exception? ex = null)
    {
        try
        {
            var line = $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff}] {message}";
            if (ex is not null)
            {
                line += Environment.NewLine + ex;
            }
            line += Environment.NewLine + new string('-', 60) + Environment.NewLine;
            lock (Gate)
            {
                File.AppendAllText(LogPath, line);
            }
            Debug.WriteLine(line);
        }
        catch
        {
            // never throw from logger
        }
    }
}
