package com.atvoptimizer.mobile.adb

import android.content.Context
import android.os.Build
import io.github.muntashirakon.adb.AbsAdbConnectionManager
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder
import org.bouncycastle.jce.provider.BouncyCastleProvider
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder
import java.io.File
import java.math.BigInteger
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.Security
import java.security.cert.Certificate
import java.security.cert.CertificateFactory
import java.security.spec.PKCS8EncodedKeySpec
import java.util.Date
import java.util.concurrent.TimeUnit

class AtvAdbConnectionManager private constructor(private val context: Context) : AbsAdbConnectionManager() {
  private val privateKey: PrivateKey
  private val certificate: Certificate

  init {
    setApi(Build.VERSION.SDK_INT)
    setTimeout(30_000, TimeUnit.MILLISECONDS)

    val storedKey = readPrivateKey()
    val storedCert = readCertificate()
    if (storedKey != null && storedCert != null) {
      privateKey = storedKey
      certificate = storedCert
    } else {
      val pair = KeyPairGenerator.getInstance("RSA").apply {
        initialize(2048, SecureRandom())
      }.generateKeyPair()
      privateKey = pair.private
      certificate = buildCertificate(pair.public, pair.private)
      keyFile.writeBytes(privateKey.encoded)
      certFile.writeBytes(certificate.encoded)
    }
  }

  override fun getPrivateKey(): PrivateKey = privateKey

  override fun getCertificate(): Certificate = certificate

  override fun getDeviceName(): String = "ATV Optimizer"

  private fun readPrivateKey(): PrivateKey? = runCatching {
    if (!keyFile.isFile) return null
    val spec = PKCS8EncodedKeySpec(keyFile.readBytes())
    KeyFactory.getInstance("RSA").generatePrivate(spec)
  }.getOrNull()

  private fun readCertificate(): Certificate? = runCatching {
    if (!certFile.isFile) return null
    certFile.inputStream().use { CertificateFactory.getInstance("X.509").generateCertificate(it) }
  }.getOrNull()

  private val keyFile: File
    get() = File(context.filesDir, "atv-adb-private.pk8")

  private val certFile: File
    get() = File(context.filesDir, "atv-adb-cert.der")

  companion object {
    @Volatile private var instance: AtvAdbConnectionManager? = null

    fun getInstance(context: Context): AtvAdbConnectionManager =
      instance ?: synchronized(this) {
        instance ?: AtvAdbConnectionManager(context.applicationContext).also { instance = it }
      }

    private fun buildCertificate(publicKey: java.security.PublicKey, privateKey: PrivateKey): Certificate {
      if (Security.getProvider(BouncyCastleProvider.PROVIDER_NAME) == null) {
        Security.addProvider(BouncyCastleProvider())
      }
      val now = System.currentTimeMillis()
      val subject = X500Name("CN=ATV Optimizer")
      val builder = JcaX509v3CertificateBuilder(
        subject,
        BigInteger.valueOf(now),
        Date(now - 60_000),
        Date(now + 10L * 365 * 24 * 60 * 60 * 1000),
        subject,
        publicKey,
      )
      val signer = JcaContentSignerBuilder("SHA256withRSA").build(privateKey)
      return JcaX509CertificateConverter()
        .setProvider(BouncyCastleProvider.PROVIDER_NAME)
        .getCertificate(builder.build(signer))
    }
  }
}
