using System.Threading;
using AsignacionDelCielo_WinUI.Services;
using Microsoft.UI.Xaml;

namespace AsignacionDelCielo_WinUI;

public partial class App : Application
{
    private Window? _window;
    private static Mutex? _singleInstance;

    /// <summary>Process-lifetime API host (survives page navigations).</summary>
    public static ApiHost? SharedApi { get; set; }

    public App()
    {
        // Log before anything else — WER crashes often had no log line.
        CrashLog.Write("App ctor begin");

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

        try
        {
            InitializeComponent();
            CrashLog.Write("App starting (InitializeComponent ok)");
        }
        catch (Exception ex)
        {
            CrashLog.Write("InitializeComponent failed", ex);
            throw;
        }
    }

    protected override async void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
    {
        // Second instance often crashes on WebView2 profile lock — exit cleanly.
        const string mutexName = "Local\\AsignacionDelCielo.WinUI.SingleInstance";
        try
        {
            _singleInstance = new Mutex(initiallyOwned: true, name: mutexName, createdNew: out var created);
            if (!created)
            {
                CrashLog.Write("Second instance detected — exiting without crash");
                _singleInstance.Dispose();
                _singleInstance = null;
                Environment.Exit(0);
                return;
            }
        }
        catch (Exception ex)
        {
            CrashLog.Write("Single-instance mutex failed (continuing)", ex);
        }

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

        try
        {
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
                try
                {
                    _singleInstance?.ReleaseMutex();
                    _singleInstance?.Dispose();
                }
                catch
                {
                    // ignore
                }
                _singleInstance = null;
            };
            _window.Activate();
            CrashLog.Write("MainWindow activated");
        }
        catch (Exception ex)
        {
            CrashLog.Write("MainWindow create/activate failed", ex);
            throw;
        }
    }
}
