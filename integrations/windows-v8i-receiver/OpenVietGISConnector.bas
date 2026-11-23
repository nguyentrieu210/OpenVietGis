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

' Launch the native V8i DXF file-open/import selector; user must select DXF
' and confirm read-only. This key-in does not inject geometry into an open DGN.
' Test on the actual installed V8i version before relying on the command.
Public Sub OpenVietGIS_OpenDXFDialog()
    On Error GoTo Failed
    If MsgBox("Open a received DXF as READ-ONLY in a separate file. " & _
              "Save your active DGN first. Continue?", _
              vbYesNo + vbQuestion, "OpenVietGIS") <> vbYes Then Exit Sub
    CadInputQueue.SendKeyin "IMPORT DXFFILE"
    Exit Sub
Failed:
    MsgBox "V8i DXF dialog not available on this installation.", vbExclamation, "OpenVietGIS"
End Sub