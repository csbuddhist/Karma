package com.karma.android

import com.karma.android.AppLanguage.Choice
import org.junit.Assert.assertEquals
import org.junit.Test

class AppLanguageTest {
    @Test
    fun empty_or_missing_tag_follows_the_system() {
        assertEquals(Choice.SYSTEM, Choice.fromTag(null))
        assertEquals(Choice.SYSTEM, Choice.fromTag(""))
    }

    @Test
    fun english_and_chinese_tags_map_to_their_choice() {
        assertEquals(Choice.ENGLISH, Choice.fromTag("en"))
        assertEquals(Choice.ENGLISH, Choice.fromTag("en-GB"))
        assertEquals(Choice.CHINESE, Choice.fromTag("zh-CN"))
        assertEquals(Choice.CHINESE, Choice.fromTag("zh-Hans-CN"))
        assertEquals(Choice.CHINESE, Choice.fromTag("zh_CN"))
    }

    @Test
    fun first_tag_of_a_list_decides() {
        assertEquals(Choice.CHINESE, Choice.fromTag("zh-CN,en"))
    }

    @Test
    fun unsupported_language_follows_the_system() {
        assertEquals(Choice.SYSTEM, Choice.fromTag("fr-FR"))
    }

    @Test
    fun every_choice_round_trips_through_its_tag() {
        Choice.entries.forEach { assertEquals(it, Choice.fromTag(it.tag)) }
    }
}
