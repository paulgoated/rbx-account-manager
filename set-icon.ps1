# Embeds an .ico into an .exe (RT_ICON + RT_GROUP_ICON resources) using the Windows API.
param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string]$Ico)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
public static class IconRes {
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr BeginUpdateResource(string file, bool deleteExisting);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool UpdateResource(IntPtr h, IntPtr type, IntPtr name, ushort lang, byte[] data, uint size);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool EndUpdateResource(IntPtr h, bool discard);

    public static void Apply(string exe, string ico) {
        byte[] f = File.ReadAllBytes(ico);
        int count = BitConverter.ToUInt16(f, 4);
        IntPtr h = BeginUpdateResource(exe, false);
        if (h == IntPtr.Zero) throw new Exception("BeginUpdateResource failed: " + Marshal.GetLastWin32Error());
        var grp = new MemoryStream();
        var w = new BinaryWriter(grp);
        w.Write((ushort)0); w.Write((ushort)1); w.Write((ushort)count);
        for (int i = 0; i < count; i++) {
            int e = 6 + i * 16;
            int size = BitConverter.ToInt32(f, e + 8);
            int off = BitConverter.ToInt32(f, e + 12);
            byte[] img = new byte[size];
            Array.Copy(f, off, img, 0, size);
            ushort id = (ushort)(i + 1);
            if (!UpdateResource(h, (IntPtr)3, (IntPtr)id, 0, img, (uint)size))
                throw new Exception("UpdateResource RT_ICON failed: " + Marshal.GetLastWin32Error());
            w.Write(f, e, 12);      // width,height,colors,reserved,planes,bitcount,bytes
            w.Write(id);            // nID
        }
        byte[] g = grp.ToArray();
        if (!UpdateResource(h, (IntPtr)14, (IntPtr)1, 0, g, (uint)g.Length))
            throw new Exception("UpdateResource RT_GROUP_ICON failed: " + Marshal.GetLastWin32Error());
        if (!EndUpdateResource(h, false)) throw new Exception("EndUpdateResource failed: " + Marshal.GetLastWin32Error());
    }
}
'@
# Antivirus software sometimes still locks a freshly copied .exe, so retry a few times.
for ($i = 1; $i -le 10; $i++) {
    try { [IconRes]::Apply((Resolve-Path $Exe).Path, (Resolve-Path $Ico).Path); "Icon embedded."; return }
    catch { if ($i -eq 10) { throw }; Start-Sleep -Milliseconds 800 }
}
