using System.Diagnostics;
using AsignacionDelCielo_WinUI.Services;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Web.WebView2.Core;
using Windows.UI;

namespace AsignacionDelCielo_WinUI;

/// <summary>
/// Hosts the existing <c>ui/index.html</c> study UI inside WebView2,
/// with the Rust e-Sword API as a local sidecar.
/// </summary>
public sealed partial class MainPage : Page
{
    private ApiHost? _api;
    private const string VirtualHost = "app.asignacion.local";

    public MainPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        try
        {
            SetBoot("Arrancando API e-Sword…");
            _api = new ApiHost();
            await _api.EnsureRunningAsync();

            SetBoot("Preparando WebView2…");
            await AppWebView.EnsureCoreWebView2Async();

            var core = AppWebView.CoreWebView2;
            core.Settings.IsStatusBarEnabled = false;
            core.Settings.AreDefaultContextMenusEnabled = true;
            core.Settings.IsZoomControlEnabled = true;

            // Transparent page so Acrylic/Mica from the window shows through CSS glass layers.
            AppWebView.DefaultBackgroundColor = Color.FromArgb(0, 0, 0, 0);

            var uiFolder = ResolveUiFolder();
            if (uiFolder is null || !File.Exists(Path.Combine(uiFolder, "index.html")))
            {
                throw new FileNotFoundException(
                    "No se encontró ui/index.html. Ejecutá desde el repo o copiá la carpeta ui junto al .exe.");
            }

            core.SetVirtualHostNameToFolderMapping(
                VirtualHost,
                uiFolder,
                CoreWebView2HostResourceAccessKind.Allow);

            // Inject host bridge before any page script runs.
            var bootstrap = $@"
                window.__ADC_HOST__ = 'winui3';
                window.__ADC_API_BASE__ = '{_api.BaseUrl}';
                console.info('[ADC] WinUI3 host bridge ready', window.__ADC_API_BASE__);
            ";
            await core.AddScriptToExecuteOnDocumentCreatedAsync(bootstrap);

            core.NavigationCompleted += (_, args) =>
            {
                if (args.IsSuccess)
                {
                    HideBoot();
                }
                else
                {
                    SetBoot($"Error de navegación: {args.WebErrorStatus}", error: true);
                }
            };

            SetBoot("Cargando interfaz…");
            AppWebView.Source = new Uri($"https://{VirtualHost}/index.html");
        }
        catch (Exception ex)
        {
            Debug.WriteLine(ex);
            SetBoot(ex.Message, error: true);
        }
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        _api?.Dispose();
        _api = null;
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

    /// <summary>
    /// Finds the repo <c>ui/</c> folder or a copied content folder next to the exe.
    /// </summary>
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
