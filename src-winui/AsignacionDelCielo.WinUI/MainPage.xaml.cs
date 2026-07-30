using AsignacionDelCielo_WinUI.Services;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Web.WebView2.Core;
using Windows.UI;

namespace AsignacionDelCielo_WinUI;

/// <summary>
/// Minimal WebView2 host. Prefer scripts/run-stable.ps1 (Edge app) if this crashes.
/// Loads UI from adc-api http://127.0.0.1:17865/ (same origin as API).
/// </summary>
public sealed partial class MainPage : Page
{
    public MainPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        try
        {
            await InitAsync();
        }
        catch (Exception ex)
        {
            CrashLog.Write("MainPage.OnLoaded failed", ex);
            SetBoot(ex.Message, error: true);
        }
    }

    private async Task InitAsync()
    {
        SetBoot("Conectando API…");
        var api = App.SharedApi;
        if (api is null)
        {
            api = new ApiHost();
            App.SharedApi = api;
            await api.EnsureRunningAsync();
        }
        else if (!await api.IsHealthyAsync())
        {
            await api.EnsureRunningAsync();
        }

        SetBoot("Preparando WebView2…");

        // Isolated profile + disable GPU — cuts a common native crash class.
        var userData = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "asignacion-del-cielo-bible",
            "webview2-profile");
        Directory.CreateDirectory(userData);

        var options = new CoreWebView2EnvironmentOptions(
            additionalBrowserArguments: "--disable-gpu --disable-gpu-compositing --disable-features=CalculateNativeWinOcclusion");
        var env = await CoreWebView2Environment.CreateAsync(
            browserExecutableFolder: null,
            userDataFolder: userData,
            options: options);

        await AppWebView.EnsureCoreWebView2Async(env);

        var core = AppWebView.CoreWebView2;
        core.Settings.IsStatusBarEnabled = false;
        core.Settings.AreDefaultContextMenusEnabled = true;
        core.Settings.IsZoomControlEnabled = true;
        AppWebView.DefaultBackgroundColor = Color.FromArgb(255, 11, 15, 22);

        core.ProcessFailed += (_, args) =>
        {
            CrashLog.Write(
                $"WebView2 ProcessFailed kind={args.ProcessFailedKind} reason={args.Reason} exit={args.ExitCode}");
            DispatcherQueue.TryEnqueue(() =>
                SetBoot(
                    "WebView2 crasheó. Usá el shell estable: .\\scripts\\run-stable.ps1",
                    error: true));
        };

        core.NavigationCompleted += (_, args) =>
        {
            if (args.IsSuccess)
            {
                HideBoot();
                CrashLog.Write("WebView navigation OK (http UI)");
            }
            else
            {
                CrashLog.Write($"WebView nav failed: {args.WebErrorStatus}");
                SetBoot($"Navegación falló: {args.WebErrorStatus}", error: true);
            }
        };

        // Same-origin UI served by adc-api (bridge injected server-side).
        SetBoot("Cargando interfaz…");
        var url = api.BaseUrl.TrimEnd('/') + "/";
        CrashLog.Write($"Navigating to {url}");
        AppWebView.Source = new Uri(url);
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
}
