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

        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);

        try
        {
            AppWindow.SetIcon("Assets/AppIcon.ico");
        }
        catch (Exception ex)
        {
            CrashLog.Write("SetIcon failed", ex);
        }

        var size = new SizeInt32(1280, 800);
        AppWindow.Resize(size);

        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = true;
            presenter.IsMaximizable = true;
            presenter.IsMinimizable = true;
        }

        RootFrame.Navigate(typeof(MainPage));
    }
}
