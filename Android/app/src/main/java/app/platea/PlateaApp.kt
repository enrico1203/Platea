package app.platea

import android.app.Application
import android.app.UiModeManager
import android.content.Context
import android.content.pm.PackageManager
import android.content.res.Configuration
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import app.platea.data.Api
import app.platea.data.Session
import coil.ImageLoader
import coil.ImageLoaderFactory
import coil.disk.DiskCache
import coil.memory.MemoryCache

class PlateaApp : Application(), ImageLoaderFactory {
    override fun onCreate() {
        super.onCreate()
        Api.init(this, isTv(this))
        Session.start()
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) = Session.onForeground()
        })
    }

    /** Immagini dal server con la stessa connessione dell'API, cache in memoria e su disco. */
    override fun newImageLoader(): ImageLoader = ImageLoader.Builder(this)
        .okHttpClient { Api.http }
        .memoryCache { MemoryCache.Builder(this).maxSizePercent(0.2).build() }
        .diskCache { DiskCache.Builder().directory(cacheDir.resolve("images")).maxSizeBytes(256L * 1024 * 1024).build() }
        .crossfade(true)
        .respectCacheHeaders(false)
        .build()

    companion object {
        /** Android TV, Google TV e Fire TV. */
        fun isTv(context: Context): Boolean {
            val ui = context.getSystemService(Context.UI_MODE_SERVICE) as? UiModeManager
            if (ui?.currentModeType == Configuration.UI_MODE_TYPE_TELEVISION) return true
            val pm = context.packageManager
            return pm.hasSystemFeature(PackageManager.FEATURE_LEANBACK) ||
                pm.hasSystemFeature("amazon.hardware.fire_tv") ||
                !pm.hasSystemFeature(PackageManager.FEATURE_TOUCHSCREEN)
        }
    }
}
