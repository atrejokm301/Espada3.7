using AsignacionDelCielo_WinUI.Services;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Web.WebView2.Core;
using Windows.UI;

namespace AsignacionDelCielo_WinUI;

/// <summary>
/// Hosts <c>ui/index.html</c> in WebView2. Stability-first: opaque background,
/// process-failed recovery, shared API host (not killed on page unload).
/// </summary>
public sealed partial class MainPage : Page
{
    private const string VirtualHost = "app.asignacion.local";
    private bool _webReady;
    private string? _uiFolder;

    public MainPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        try
        {
            await InitWebViewAsync();
        }
        catch (Exception ex)
        {
            CrashLog.Write("MainPage.OnLoaded failed", ex);
            SetBoot(ex.Message, error: true);
        }
    }

    private async Task InitWebViewAsync()
    {
        SetBoot("Conectando API e-Sword…");
        var api = App.SharedApi;
        if (api is null)
        {
            api = new ApiHost();
            App.SharedApi = api; // best-effort if launch path skipped
            await api.EnsureRunningAsync();
        }
        else if (!await api.IsHealthyAsync())
        {
            await api.EnsureRunningAsync();
        }

        SetBoot("Preparando WebView2…");
        await AppWebView.EnsureCoreWebView2Async();

        var core = AppWebView.CoreWebView2;
        core.Settings.IsStatusBarEnabled = false;
        core.Settings.AreDefaultContextMenusEnabled = true;
        core.Settings.IsZoomControlEnabled = true;

        // Opaque dark — transparent WebView2 + system backdrop was crashing on this machine.
        AppWebView.DefaultBackgroundColor = Color.FromArgb(255, 11, 15, 22);

        _uiFolder = ResolveUiFolder();
        if (_uiFolder is null || !File.Exists(Path.Combine(_uiFolder, "index.html")))
        {
            throw new FileNotFoundException(
                "No se encontró ui/index.html. Ejecutá desde el repo o copiá la carpeta ui junto al .exe.");
        }

        core.SetVirtualHostNameToFolderMapping(
            VirtualHost,
            _uiFolder,
            CoreWebView2HostResourceAccessKind.Allow);

        // Avoid stacking bootstrap scripts on re-init
        if (!_webReady)
        {
            var bootstrap = $@"
                window.__ADC_HOST__ = 'winui3';
                window.__ADC_API_BASE__ = '{api.BaseUrl}';
                window.__ADC_STABLE__ = true;
                console.info('[ADC] WinUI3 host bridge ready', window.__ADC_API_BASE__);
            ";
            await core.AddScriptToExecuteOnDocumentCreatedAsync(bootstrap);
        }

        core.NavigationCompleted -= OnNavigationCompleted;
        core.NavigationCompleted += OnNavigationCompleted;

        core.ProcessFailed -= OnWebProcessFailed;
        core.ProcessFailed += OnWebProcessFailed;

        SetBoot("Cargando interfaz…");
        AppWebView.Source = new Uri($"https://{VirtualHost}/index.html?t={DateTimeOffset.UtcNow.ToUnixTimeSeconds()}");
        _webReady = true;
        CrashLog.Write("WebView navigation started");
    }

    private void OnNavigationCompleted(CoreWebView2 sender, CoreWebView2NavigationCompletedEventArgs args)
    {
        if (args.IsSuccess)
        {
            HideBoot();
            CrashLog.Write("WebView navigation OK");
        }
        else
        {
            CrashLog.Write($"WebView navigation failed: {args.WebErrorStatus}");
            SetBoot($"Error de navegación: {args.WebErrorStatus}", error: true);
        }
    }

    private async void OnWebProcessFailed(CoreWebView2 sender, CoreWebView2ProcessFailedEventArgs args)
    {
        CrashLog.Write(
            $"WebView2 ProcessFailed kind={args.ProcessFailedKind} reason={args.Reason} exit={args.ExitCode}");

        // Recover instead of taking down the whole WinUI process.
        try
        {
            SetBoot("WebView se reinició… recargando…", error: false);
            await Task.Delay(400);
            DispatcherQueue.TryEnqueue(async () =>
            {
                try
                {
                    await InitWebViewAsync();
                }
                catch (Exception ex)
                {
                    CrashLog.Write("WebView recovery failed", ex);
                    SetBoot("Falló la recuperación de WebView: " + ex.Message, error: true);
                }
            });
        }
        catch (Exception ex)
        {
            CrashLog.Write("OnWebProcessFailed handler error", ex);
        }
    }

    private void SetBoot(string message, bool error = false)
    {
        BootOverlay.Visibility = Visibility.Visible;
        BootStatus.Text = message;
        BootStatus.Foreground = new Microsoft.UI.Xaml.Media.SolidColorBrush(
            error
                ? Color.FromArgb(255, 248, 113, 113)
                : Color.FromArgb(255, 243, 246, 251));
    }

    private void HideBoot()
    {
        BootOverlay.Visibility = Visibility.Collapsed;
    }

    private static string? ResolveUiFolder()
    {
        var env = Environment.GetEnvironmentVariable("ADC_UI_DIR");
        if (!string.IsNullOrWhiteSpace(env) && Directory.Exists(env))
        {
            return Path.GetFullPath(env);
        }

        var candidates = new List<string>
        {
            Path.Combine(AppContext.BaseDirectory, "ui"),
            Path.Combine(AppContext.BaseDirectory, "wwwroot"),
        };

        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        for (var i = 0; i < 10 && dir is not null; i++)
        {
            candidates.Add(Path.Combine(dir.FullName, "ui"));
            dir = dir.Parent;
        }

        return candidates.FirstOrDefault(d => File.Exists(Path.Combine(d, "index.html")));
    }
}
