using AsignacionDelCielo_WinUI.Services;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Windows.Graphics;

namespace AsignacionDelCielo_WinUI;

public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();

        // Standard OS title bar only — custom chrome was part of the crash surface.
        ExtendsContentIntoTitleBar = false;

        try
        {
            AppWindow.SetIcon("Assets/AppIcon.ico");
        }
        catch (Exception ex)
        {
            CrashLog.Write("SetIcon failed", ex);
        }

        AppWindow.Resize(new SizeInt32(1280, 800));
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = true;
            presenter.IsMaximizable = true;
            presenter.IsMinimizable = true;
        }

        RootFrame.Navigate(typeof(MainPage));
    }
}
