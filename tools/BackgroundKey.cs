using System;
using System.Collections.Generic;

/// <summary>
/// Removes the flat backdrop from artwork without eating dark subject matter.
///
/// A plain luminance threshold cannot do this job here: the portal's stonework
/// is as dark as the field behind it, so any cutoff that erases the background
/// also erases the columns. What actually separates them is connectivity — the
/// background touches the image border and the stone does not.
///
/// So: flood fill inward from the edges through dark pixels only. Anything the
/// fill reaches is backdrop; anything it cannot reach is subject, however dark.
/// Alpha then ramps with luminance inside the filled region, which keeps the
/// portal's glow fading out softly instead of stopping at a hard line.
/// </summary>
public static class BackgroundKey
{
    /// <param name="buf">32bpp ARGB pixels, little-endian: B, G, R, A.</param>
    /// <param name="floodMax">Fill always spreads through pixels dimmer than this.</param>
    /// <param name="chromaMin">
    /// Blue excess (B minus R) above which a pixel counts as the portal's bloom
    /// rather than stonework. The bloom is strongly blue; the stone is close to
    /// neutral grey, so this separates the two where brightness cannot.
    /// </param>
    /// <param name="bloomMax">Upper luminance for bloom pixels, so the fill
    /// stops at the portal's bright interior instead of eating it.</param>
    /// <param name="feather">Box-blur radius applied to the alpha channel, to
    /// soften the cut edge. 0 disables it.</param>
    public static void Apply(
        byte[] buf, int width, int height, int stride,
        int floodMax, int chromaMin, int bloomMax, int feather)
    {
        int n = width * height;
        byte[] lum = new byte[n];
        short[] chroma = new short[n];

        for (int y = 0; y < height; y++)
        {
            int row = y * stride;
            int outRow = y * width;
            for (int x = 0; x < width; x++)
            {
                int i = row + x * 4;
                int b = buf[i], g = buf[i + 1], r = buf[i + 2];
                lum[outRow + x] = (byte)((r * 299 + g * 587 + b * 114) / 1000);
                chroma[outRow + x] = (short)(b - r);
            }
        }

        bool[] outside = new bool[n];
        Stack<int> stack = new Stack<int>(n / 4);
        Key k = new Key(lum, chroma, floodMax, chromaMin, bloomMax);

        // Seed from every border pixel that reads as backdrop.
        for (int x = 0; x < width; x++)
        {
            Seed(stack, outside, k, x);
            Seed(stack, outside, k, (height - 1) * width + x);
        }
        for (int y = 0; y < height; y++)
        {
            Seed(stack, outside, k, y * width);
            Seed(stack, outside, k, y * width + width - 1);
        }

        // Four-way flood. An explicit stack, not recursion — a megapixel of
        // background would blow the call stack.
        while (stack.Count > 0)
        {
            int p = stack.Pop();
            int px = p % width, py = p / width;

            if (px > 0) Seed(stack, outside, k, p - 1);
            if (px < width - 1) Seed(stack, outside, k, p + 1);
            if (py > 0) Seed(stack, outside, k, p - width);
            if (py < height - 1) Seed(stack, outside, k, p + width);
        }

        // Anything the fill reached is backdrop and goes fully transparent.
        // Deriving alpha from luminance instead would undo the whole exercise:
        // a mid-bright background pixel would come back nearly opaque.
        byte[] alpha = new byte[n];
        for (int i = 0; i < n; i++) alpha[i] = outside[i] ? (byte)0 : (byte)255;

        if (feather > 0) Feather(alpha, width, height, feather);

        for (int y = 0; y < height; y++)
        {
            int row = y * stride;
            int aRow = y * width;
            for (int x = 0; x < width; x++)
                buf[row + x * 4 + 3] = alpha[aRow + x];
        }
    }

    /// Separable box blur over the alpha channel only, so the cut edge is
    /// antialiased rather than stair-stepped. Colour is left untouched.
    private static void Feather(byte[] alpha, int width, int height, int radius)
    {
        int n = width * height;
        int window = radius * 2 + 1;
        byte[] tmp = new byte[n];

        for (int y = 0; y < height; y++)
        {
            int row = y * width;
            for (int x = 0; x < width; x++)
            {
                int sum = 0;
                for (int d = -radius; d <= radius; d++)
                {
                    int xx = x + d;
                    if (xx < 0) xx = 0; else if (xx >= width) xx = width - 1;
                    sum += alpha[row + xx];
                }
                tmp[row + x] = (byte)(sum / window);
            }
        }

        for (int x = 0; x < width; x++)
        {
            for (int y = 0; y < height; y++)
            {
                int sum = 0;
                for (int d = -radius; d <= radius; d++)
                {
                    int yy = y + d;
                    if (yy < 0) yy = 0; else if (yy >= height) yy = height - 1;
                    sum += tmp[yy * width + x];
                }
                alpha[y * width + x] = (byte)(sum / window);
            }
        }
    }

    private static void Seed(Stack<int> stack, bool[] outside, Key k, int p)
    {
        if (outside[p] || !k.IsBackdrop(p)) return;
        outside[p] = true;
        stack.Push(p);
    }

    /// Decides whether a single pixel can be flooded through.
    private struct Key
    {
        private readonly byte[] _lum;
        private readonly short[] _chroma;
        private readonly int _floodMax, _chromaMin, _bloomMax;

        public Key(byte[] lum, short[] chroma, int floodMax, int chromaMin, int bloomMax)
        {
            _lum = lum; _chroma = chroma;
            _floodMax = floodMax; _chromaMin = chromaMin; _bloomMax = bloomMax;
        }

        public bool IsBackdrop(int p)
        {
            int l = _lum[p];
            if (l < _floodMax) return true;                       // plain dark field
            return _chroma[p] > _chromaMin && l < _bloomMax;      // blue bloom
        }
    }
}
