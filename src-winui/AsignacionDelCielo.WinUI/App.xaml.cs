using AsignacionDelCielo_WinUI.Services;
using Microsoft.UI.Xaml;

namespace AsignacionDelCielo_WinUI;

public partial class App : Application
{
    private Window? _window;

    /// <summary>Process-lifetime API host (survives page navigations).</summary>
    public static ApiHost? SharedApi { get; set; }

    public App()
    {
        // Catch managed crashes so we get a log instead of a silent death.
        UnhandledException += (_, e) =>
        {
            CrashLog.Write("App.UnhandledException", e.Exception);
            e.Handled = true; // try to keep the process alive when possible
        };
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
        {
            CrashLog.Write(
                "AppDomain.UnhandledException",
                e.ExceptionObject as Exception);
        };
        TaskScheduler.UnobservedTaskException += (_, e) =>
        {
            CrashLog.Write("TaskScheduler.UnobservedTaskException", e.Exception);
            e.SetObserved();
        };

        InitializeComponent();
        CrashLog.Write("App starting");
    }

    protected override async void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
    {
        try
        {
            SharedApi = new ApiHost();
            await SharedApi.EnsureRunningAsync();
            CrashLog.Write($"adc-api ready at {SharedApi.BaseUrl}");
        }
        catch (Exception ex)
        {
            CrashLog.Write("Failed to start adc-api at launch", ex);
            // Still open the window so the user sees the error overlay.
        }

        _window = new MainWindow();
        _window.Closed += (_, _) =>
        {
            CrashLog.Write("MainWindow closed");
            try
            {
                SharedApi?.Dispose();
            }
            catch (Exception ex)
            {
                CrashLog.Write("SharedApi dispose failed", ex);
            }
            SharedApi = null;
        };
        _window.Activate();
    }
}
