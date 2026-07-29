using System.Diagnostics;
using System.Net.Http;
using System.Net.Sockets;

namespace AsignacionDelCielo_WinUI.Services;

/// <summary>
/// Starts and health-checks the Rust <c>adc-api</c> sidecar that serves e-Sword commands.
/// </summary>
public sealed class ApiHost : IDisposable
{
    public const int DefaultPort = 17865;
    public string BaseUrl { get; }
    public int Port { get; }

    private Process? _process;
    private readonly HttpClient _http = new() { Timeout = TimeSpan.FromSeconds(3) };
    private bool _ownedProcess;

    public ApiHost(int port = DefaultPort)
    {
        Port = port;
        BaseUrl = $"http://127.0.0.1:{port}";
    }

    public async Task EnsureRunningAsync(CancellationToken ct = default)
    {
        if (await IsHealthyAsync(ct).ConfigureAwait(false))
        {
            return;
        }

        var exe = ResolveApiExecutable();
        if (exe is null)
        {
            throw new InvalidOperationException(
                "No se encontró adc-api.exe. Compilá el sidecar:\n" +
                "  cd src-tauri\n" +
                "  cargo build --bin adc-api\n" +
                "O ejecutá scripts\\run-winui.ps1 que lo construye automáticamente.");
        }

        var psi = new ProcessStartInfo
        {
            FileName = exe,
            UseShellExecute = false,
            CreateNoWindow = true,
            WindowStyle = ProcessWindowStyle.Hidden,
            WorkingDirectory = Path.GetDirectoryName(exe) ?? Environment.CurrentDirectory,
        };
        psi.Environment["ADC_API_PORT"] = Port.ToString();

        _process = Process.Start(psi)
            ?? throw new InvalidOperationException($"No se pudo iniciar {exe}");
        _ownedProcess = true;

        var deadline = DateTime.UtcNow.AddSeconds(20);
        while (DateTime.UtcNow < deadline)
        {
            ct.ThrowIfCancellationRequested();
            if (await IsHealthyAsync(ct).ConfigureAwait(false))
            {
                return;
            }
            await Task.Delay(200, ct).ConfigureAwait(false);
        }

        throw new TimeoutException("adc-api no respondió /health a tiempo.");
    }

    public async Task<bool> IsHealthyAsync(CancellationToken ct = default)
    {
        try
        {
            using var res = await _http.GetAsync($"{BaseUrl}/health", ct).ConfigureAwait(false);
            return res.IsSuccessStatusCode;
        }
        catch (HttpRequestException)
        {
            return false;
        }
        catch (TaskCanceledException)
        {
            return false;
        }
        catch (SocketException)
        {
            return false;
        }
    }

    private static string? ResolveApiExecutable()
    {
        // 1) Next to the WinUI exe (release layout / copy step)
        var baseDir = AppContext.BaseDirectory;
        var candidates = new List<string>
        {
            Path.Combine(baseDir, "adc-api.exe"),
            Path.Combine(baseDir, "api", "adc-api.exe"),
        };

        // 2) Repo layout: src-winui/... → repo root → src-tauri/target/{debug,release}
        var dir = new DirectoryInfo(baseDir);
        for (var i = 0; i < 8 && dir is not null; i++)
        {
            var tauri = Path.Combine(dir.FullName, "src-tauri", "target");
            candidates.Add(Path.Combine(tauri, "release", "adc-api.exe"));
            candidates.Add(Path.Combine(tauri, "debug", "adc-api.exe"));
            // When running from src-winui/AsignacionDelCielo.WinUI/bin/...
            candidates.Add(Path.Combine(dir.FullName, "target", "release", "adc-api.exe"));
            candidates.Add(Path.Combine(dir.FullName, "target", "debug", "adc-api.exe"));
            dir = dir.Parent;
        }

        // 3) Explicit env override
        var envPath = Environment.GetEnvironmentVariable("ADC_API_EXE");
        if (!string.IsNullOrWhiteSpace(envPath))
        {
            candidates.Insert(0, envPath);
        }

        return candidates.FirstOrDefault(File.Exists);
    }

    public void Dispose()
    {
        _http.Dispose();
        if (_ownedProcess && _process is { HasExited: false })
        {
            try
            {
                _process.Kill(entireProcessTree: true);
            }
            catch
            {
                // best-effort shutdown
            }
        }
        _process?.Dispose();
    }
}
