package net.sandrakessler.seesaw.rules;

import com.fasterxml.jackson.databind.JsonNode;

import java.util.ArrayList;
import java.util.List;


/**
 * Transformationsketten aus der Formatdarstellung.
 *
 * <p>Die Kettenmechanik selbst — anwenden, invertieren, normalisieren,
 * {@code identBytes} — liegt in {@link Chain} und ist gegen die
 * Rust-Seite golden-geprüft. Diese Klasse macht nur das, was das Regelformat
 * hinzufügt: aus einer JSON-Liste eine Kette bauen, und zwar mit
 * derselben Strenge wie serde in {@code rules/format.rs}.
 *
 * <p>Fünf Primitive stehen im Format (Spec §5): {@code identity},
 * {@code capitalize}, {@code decapitalize}, {@code prefix},
 * {@code suffix}. Die beiden Umkehr-Primitive {@code strip_prefix} und
 * {@code strip_suffix} entstehen nur aus {@code inverse()} und sind
 * aus dem Format NICHT schreibbar — Rusts {@code PrimDecl} kennt sie
 * nicht, und dieser Leser lehnt sie deshalb ab.
 */
public final class Transform {
    private Transform() {}

    /** Kette aus dem Feld {@code transform}; fehlend oder leer = Identität. */
    public static Chain readChain(JsonNode arr, Json.At at) {
        if (arr == null || arr.isNull()) return Chain.IDENTITY;
        if (!arr.isArray()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "field 'transform' must be an array of primitives");
        }
        List<Prim> prims = new ArrayList<>();
        for (JsonNode p : arr) prims.add(readPrim(p, at));
        return Chain.chain(prims);
    }

    /**
     * Ein Primitiv. Der {@code op}-Wert bestimmt, welche Felder erlaubt
     * sind: die drei argumentlosen Arten dulden kein {@code arg}, die
     * beiden Affix-Arten verlangen eins.
     */
    public static Prim readPrim(JsonNode p, Json.At at) {
        Json.mustBeObject(p, at, "transform primitive");
        String op = Json.requireText(p, "op", at);
        switch (op) {
            case "identity":
                Json.allowOnly(p, at, "op");
                return new Prim(PrimOp.IDENTITY);
            case "capitalize":
                Json.allowOnly(p, at, "op");
                return new Prim(PrimOp.CAPITALIZE);
            case "decapitalize":
                Json.allowOnly(p, at, "op");
                return new Prim(PrimOp.DECAPITALIZE);
            case "prefix":
                Json.allowOnly(p, at, "op", "arg");
                return new Prim(PrimOp.PREFIX, Json.requireText(p, "arg", at));
            case "suffix":
                Json.allowOnly(p, at, "op", "arg");
                return new Prim(PrimOp.SUFFIX, Json.requireText(p, "arg", at));
            default:
                throw LoadException.malformed(at.rule, at.side, at.name,
                        "unknown transform primitive '" + op
                                + "', expected one of [identity, capitalize, "
                                + "decapitalize, prefix, suffix]");
        }
    }
}
