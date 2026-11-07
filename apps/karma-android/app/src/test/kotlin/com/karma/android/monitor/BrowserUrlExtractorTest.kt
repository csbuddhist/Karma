package com.karma.android.monitor

import com.karma.android.monitor.context.BrowserUrlExtractor
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class BrowserUrlExtractorTest {
    @Test
    fun addressBarTextYieldsOnlyTheLowercaseHost() {
        assertEquals("m.example.com", BrowserUrlExtractor.hostFromAddressBar("https://M.Example.com/a?b=c"))
        assertEquals("example.com", BrowserUrlExtractor.hostFromAddressBar("example.com/path"))
        assertEquals("example.com", BrowserUrlExtractor.hostFromAddressBar(" example.com. "))
    }

    @Test
    fun searchTermsAndOtherSchemesAreIgnored() {
        assertNull(BrowserUrlExtractor.hostFromAddressBar("how to cook rice"))
        assertNull(BrowserUrlExtractor.hostFromAddressBar("localhost"))
        assertNull(BrowserUrlExtractor.hostFromAddressBar("file:///sdcard/a.html"))
        assertNull(BrowserUrlExtractor.hostFromAddressBar(""))
    }
}
