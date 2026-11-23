Attribute VB_Name = "OpenVietGISConnector"
Option Explicit

' MicroStation V8i VBA entry point. This ONLY opens a local companion GUI.
' It never edits a DGN, sends the drawing to a server, or reads credentials.
Public Sub OpenVietGIS_Connect()
    On Error GoTo Failed
    Dim ws As Object
    Dim launcher As String
    Set ws = CreateObject("WScript.Shell")
    launcher = ws.ExpandEnvironmentStrings("%LOCALAPPDATA%") & "\OpenVietGIS\Connector\start.cmd"
    If Len(Dir$(launcher)) = 0 Then
        MsgBox "OpenVietGIS Connector is not installed. Run install.cmd first.", vbExclamation, "OpenVietGIS"
        Exit Sub
    End If
    ws.Run """" & launcher & """", 1, False
    Exit Sub
Failed:
    MsgBox "Cannot open Windows Connector. Check local installation.", vbExclamation, "OpenVietGIS"
End Sub