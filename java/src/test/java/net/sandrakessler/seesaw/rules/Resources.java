package net.sandrakessler.seesaw.rules;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.io.IOException;
import java.io.InputStream;

/** Liest eine Testressource als JSON-Baum. */
final class Resources {
    private static final ObjectMapper M = new ObjectMapper();

    private Resources() {}

    static JsonNode read(String path) throws IOException {
        try (InputStream in = Resources.class.getResourceAsStream(path)) {
            if (in == null) {
                throw new IOException("Fixture fehlt: " + path);
            }
            return M.readTree(in);
        }
    }
}
