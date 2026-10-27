# La teksto de La Kuirlibro de Legejo. Ĉiu ĉapitro estas titolo, enkonduko kaj
# listo de receptoj; recepto estas (titolo, problemo, solvo, diskuto), la
# lastaj tri kiel XHTML. build.py numeras ilin kaj faras la EPUB-on.


PREFACE = """
<p>Legejo estas biblioteko por viaj e-libroj, kiun vi mem funkciigas. Vi alŝutas viajn librojn, metas
ilin sur bretojn kaj legas ilin en la retumilo, per Kobo, en KOReader aŭ en ajna legoaplikaĵo, kiu
komprenas OPDS, kaj via loko en ĉiu libro sekvas vin de unu al alia.</p>
<p>Ĉi tiu libro venis kun via biblioteko. Ĝi estis metita sur la breton, kiam Legejo estis instalita,
por ke la unua libro, kiun vi malfermas, estu tiu, kiu klarigas la ceteron. Forigu ĝin, kiam vi ne plu
bezonas ĝin; nenio dependas de ĝi.</p>

<h2>Por kiu estas ĉi tiu libro</h2>
<p>Por du specoj de legantoj. La plej multaj ĉapitroj estas por ĉiu, kiu havas konton: kiel plenigi la
bibliotekon, trovi aferojn en ĝi, legi kaj meti librojn sur aparaton. Ĉapitro 1 kaj Ĉapitro 12 estas por
la persono, kiu funkciigas la instancon, kaj supozas terminalon kaj iom da pacienco kun Docker.</p>

<h2>Kiel ĝi estas organizita</h2>
<p>Kiel receptaro. Ĉiu recepto prezentas <em>Problemon</em>, donas <em>Solvon</em> kaj, kie
estas pli por diri, <em>Diskuton</em>. Legu ĝin unufoje de komenco ĝis fino, aŭ serĉu la unu aferon,
pro kiu vi venis.</p>

<h2>Konvencioj uzataj en ĉi tiu libro</h2>
<p><strong>Grasa teksto</strong> estas io, sur kio vi klakas aŭ tuŝetas, laŭ la nomo, kiun ĝi havas sur la ekrano.
<code>Egallarĝa teksto</code> estas io, kion vi tajpas, la nomo de agordo aŭ adreso. Nomoj de
butonoj estas donitaj tiel, kiel la Esperanta interfaco montras ilin; Legejo ankaŭ parolas la anglan, la svedan, la finnan, la germanan, la francan
kaj la hispanan, kaj Recepto 11.1 diras, kiel ŝanĝi la lingvon.</p>
<p class="note"><strong>Nedevigaj funkcioj.</strong> Pluraj partoj de Legejo estas malŝaltitaj, ĝis
iu ŝaltas ilin: la serĉoj en Open Library kaj Libris, aŭdlibroj, katalogoj de aliaj bibliotekoj, la bibliotekisto, federacio,
bretoj per privata ligilo, markoj el Kobo, la Kobo-vendejo apud Legejo. Recepto diras tion, kiam ĝi temas pri unu el ili. Se butono priskribita ĉi tie
mankas sur via ekrano, tio estas la verŝajna kialo, kaj la persono, kiu funkciigas vian instancon, estas tiu,
kiun demandi.</p>

<h2>Kio Legejo ne estas</h2>
<p>Indas scii tion frue. Legejo estas konstruita ĉirkaŭ EPUB: tio estas la formato, kiun ĝi malfermas en la
retumilo, kontrolas, riparas, konvertas por Kobo kaj laŭtlegas. PDF-dosieroj kaj bildstrioj ankaŭ malfermiĝas en la retumilo,
simple. Legejo ankaŭ tenas sian propran kopion de ĉiu
libro. Ĝi ne legas ekzistantan dosierujon de libroj surloke, kaj ĝi ne sekvas ŝanĝojn faritajn
tie. La Aldono diras pli.</p>
"""

CHAPTERS = [
("Unuaj paŝoj",
 """<p>Por la persono, kiu funkciigas la instancon. Se iu alia instalis Legejon kaj donis al vi
 konton, saltu al Ĉapitro 2.</p>""",
 [
 ("Funkciigi Legejon per Docker",
  "<p>Vi volas propran Legejon, kun kiel eble plej malmulte da ceremonio.</p>",
  """<p>Elŝutu la Compose-dosieron, startigu ĝin kaj legu la unuan pasvorton el la protokolo:</p>
<pre>curl -O https://raw.githubusercontent.com/jularboolean/legejo/main/docker-compose.yml
docker compose up -d
docker compose logs legejo | grep -A3 Bootstrap</pre>
<p>Malfermu <code>http://localhost:3000</code> kaj ensalutu kiel <code>admin</code> per la pasvorto
el la protokolo.</p>""",
  """<p>Legejo estas unu sola ujo. Libroj, kovriloj kaj la datumbazo loĝas en la
volumo <code>legejo-data</code>, muntita ĉe <code>/data</code>. Tiu volumo estas via biblioteko:
ujo startigita sen ĝi tenas ĉion en si mem kaj perdas ĝin, kiam la ujo estas
forigita.</p>
<p>La bildo estas <code>slurvdjur/legejo</code> ĉe Docker Hub, por <code>linux/amd64</code> kaj
<code>linux/arm64</code>, etikedita per la plena versio, la minora versio, la ĉefa versio kaj
<code>latest</code>. Funkcianta instanco senokupe uzas ĉirkaŭ 30&#160;MB da memoro, do Raspberry Pi
aŭ la plej malgranda VPS sufiĉas.</p>"""),

 ("Mem elekti la unuan pasvorton",
  "<p>Vi preferus ne serĉi generitan pasvorton en protokolo.</p>",
  """<p>Agordu <code>LEGEJO_ADMIN_PASSWORD</code> (kaj <code>LEGEJO_ADMIN_USER</code>, se
<code>admin</code> ne plaĉas al vi) antaŭ la unua starto.</p>""",
  """<p>Ambaŭ estas legataj nur, kiam la datumbazo estas malplena. Ŝanĝi ilin poste faras nenion; ŝanĝu
anstataŭe la pasvorton en la paĝo <strong>Konto</strong>. Ĉiu agordo povas esti legata el dosiero, se oni
aldonas <code>_FILE</code> al ĝia nomo, ekzemple
<code>LEGEJO_ADMIN_PASSWORD_FILE=/run/secrets/admin</code>, kiel la sekretoj de Docker kaj Kubernetes
estas uzotaj.</p>"""),

 ("Uzi PostgreSQL anstataŭ SQLite",
  "<p>Vi jam funkciigas PostgreSQL kaj volas la datumojn de Legejo tie.</p>",
  """<p>Komencu per <code>docker-compose.postgres.yml</code> en la deponejo, aŭ agordu
<code>DATABASE_URL</code> al <code>postgres://user:password@host/db</code>.</p>
<p>Instanco, kiu komenciĝis per SQLite, povas esti translokita:</p>
<pre>docker compose -f docker-compose.postgres.yml run --rm legejo import-sqlite /data/legejo.db</pre>""",
  """<p>SQLite bezonas neniun agordon kaj estas la ĝusta elekto por la plej multaj hejmoj. La
librodosieroj restas en <code>/data</code> ĉiuokaze; nur tio, kion Legejo scias pri ili, moviĝas al
PostgreSQL.</p>"""),

 ("Meti Legejon en la interreton",
  "<p>Vi volas atingi la bibliotekon ekster via hejmo kaj konekti legilon al ĝi.</p>",
  """<p>Metu Legejon malantaŭ inversan prokurilon, kiu finas TLS, kaj agordu
<code>LEGEJO_PUBLIC_URL</code> al la <code>https://</code>-adreso, per kiu homoj atingas ĝin.</p>""",
  """<p>Kobo-sinkronigo, ligiloj en retleteroj, federacio kaj instalado de Legejo kiel aplikaĵo ĉiuj dependas
de tio, ke tiu adreso estas ĝusta. Kun publika <code>https</code>-adreso, seancaj kuketoj estas markitaj
<code>Secure</code>; agordu <code>LEGEJO_SECURE_COOKIES=false</code> nur, se vi vere servas per simpla
HTTP.</p>
<p>Se via prokurilo metas kapon kun la adreso de la kliento, nomu ĝin en
<code>LEGEJO_CLIENT_IP_HEADER</code>, por ke la bremsado de ensalutoj (Recepto 12.7) povu distingi
klientojn. Faru tion nur, se la prokurilo ĉiam anstataŭigas la kapon.</p>
<p>Ĉe Coolify, Dokploy kaj similaj platformoj: uzu la bildon <code>slurvdjur/legejo:1</code>,
direktu vian domajnon al la ujpordo <code>3000</code> kaj muntu daŭran volumon ĉe
<code>/data</code>.</p>"""),

 ("Teni Legejon ĝisdata",
  "<p>Vi volas scii, kiam aperas nova versio, kaj kion ĝi ŝanĝas.</p>",
  """<p>En la paĝo <strong>Administrado</strong>, sub <strong>Versio</strong>, premu
<strong>Serĉi novan version</strong>. Poste elŝutu la novan bildon kaj startigu la ujon
denove:</p>
<pre>docker compose pull
docker compose up -d</pre>""",
  """<p>La kontrolo demandas GitHub pri la plej nova eldono, kiam vi premas la butonon, kaj nur tiam,
el via retumilo. La servilo mem neniam demandas iun ajn pri io ajn. Kio ŝanĝiĝis en ĉiu versio, troviĝas
en la ŝanĝprotokolo en la deponejo kaj en la paĝo de eldonoj.</p>
<p>Homoj, kiuj havas la retaplikaĵon malfermita dum ĝisdatigo, vidas malgrandan noton,
<strong>Nova versio de Legejo pretas</strong>, kun butono por reŝargi. La sekvan fojon, kiam
iu ajn malfermas Legejon post ĝisdatigo, noto diras, kiu versio ĝi nun estas, kun
<strong>Kio novas</strong> je unu klako: la ŝanĝoj ekde la versio, kiun tiu retumilo laste
vidis, legitaj el la ŝanĝprotokolo enkonstruita en Legejon mem. La paĝo Administrado havas la samon sub
<strong>Versio</strong>, por la funkcianta versio.</p>"""),

 ("Sekurkopii",
  "<p>Vi volas povi perdi la maŝinon sen perdi la bibliotekon.</p>",
  """<p>Sekurkopiu la volumon <code>/data</code>. Kun SQLite tio estas ĉio. Kun PostgreSQL
sekurkopiu ankaŭ la datumbazon.</p>""",
  """<p>Ĉiu uzanto ankaŭ povas kunpreni sian propran bibliotekon: Recepto 11.5 priskribas la eksporton,
kiu enhavas la librodosierojn, la kovrilojn kaj la katalogon kiel CSV kaj JSON.</p>"""),
 ]),

("Plenigi la bibliotekon",
 "<p>Biblioteko komenciĝas malplena, krom ĉi tiu libro. Jen kiel libroj eniras.</p>",
 [
 ("Alŝuti librojn",
  "<p>Vi havas EPUB-dosierojn en via komputilo kaj volas ilin en Legejo.</p>",
  """<p>En <strong>Mia biblioteko</strong>, premu <strong>Alŝuti librojn</strong> kaj elektu la
dosierojn, aŭ trenu ilin sur la paĝon. Pluraj samtempe estas en ordo.</p>""",
  """<p>Titolo, aŭtoro, serio, kovrilo kaj la cetero estas legataj el ĉiu dosiero, kie ĝi havas ilin.
Ĉiu dosiero ricevas linion, kiu diras al vi, kio okazis al ĝi: <em>Aldonita</em>, <em>Jam en la
biblioteko</em> aŭ <em>Malsukcesis</em> kun la kialo. La plej granda akceptata dosiero estas 200&#160;MB, krom se
tiu, kiu funkciigas la instancon, ŝanĝis <code>LEGEJO_MAX_UPLOAD_MB</code>.</p>"""),

 ("Kompreni, kio okazas al dosiero",
  "<p>La alŝuto diras, ke libro estis riparita aŭ havas rimarkojn. Vi scivolas, kio estis farita al via dosiero.</p>",
  """<p>Ĉiu EPUB estas kontrolata dum la eniro. Kio povas esti riparita, estas riparita: mankanta
enhavtabelo, lingvo, identigilo aŭ kovrila deklaro, la markilo, kiu diras, ke la dosiero estas EPUB,
kaj referencoj al dosieroj, kiuj ne ekzistas. Kio ne povas esti riparita, estas raportita: rompitaj ligiloj kaj
mankantaj bildoj. La libropaĝo montras la rezulton sub <strong>Dosierkontrolo</strong>.</p>""",
  """<p>Tio estas malgrandaj aferoj, kiuj faligas legoaplikaĵojn: dosiero sen lingvo ricevas
malĝustan dividon de vortoj kaj malĝustan voĉon dum laŭtlegado; tiu sen enhavtabelo ne
estas navigebla. Libro kun rimarkoj tamen legiĝas kiel kutime.</p>
<p>Kopiprotektitaj dosieroj (DRM) estas rifuzataj. Ili legeblas nur en la aplikaĵo, por kiu ili estis
aĉetitaj, kaj Legejo povus fari nenion kun ili.</p>"""),

 ("Trakti duoblaĵojn",
  "<p>Vi ne certas, ĉu vi jam havas libron.</p>",
  """<p>Alŝutu ĝin. Ekzakte la sama dosiero ne estas aldonata dufoje, kaj Legejo diras tion. Alia dosiero,
kiu aspektas kiel la sama libro laŭ sia ISBN, estas aldonata, kun noto, kiu montras al tiu, kiun vi
havis.</p>""",
  """<p>Du eldonoj de unu verko estas du libroj, kaj ofte vi volas ambaŭ. La noto estas tie, por ke
vi povu decidi; <strong>Fermu</strong> ĝin, se la respondo estas jes.</p>"""),

 ("Lasi dosierujon plenigi la bibliotekon",
  "<p>Vi preferus meti dosierojn en dosierujon ol alŝuti ilin permane.</p>",
  """<p>La persono, kiu funkciigas la instancon, agordas <code>LEGEJO_IMPORT_DIR</code> al skribebla
dosierujo. EPUB-, PDF- kaj CBZ-dosieroj metitaj tien, inkluzive de subdosierujoj, estas aldonataj, tuj kiam ili
ĉesis ŝanĝiĝi.</p>""",
  """<p>Poste ĉiu dosiero estas movata al <code>imported/</code>, <code>duplicates/</code> aŭ
<code>failed/</code> ene de la dosierujo, por ke vi vidu, kio fariĝis el ĝi. La libroj iras al la
plej malnova administranto, krom se <code>LEGEJO_IMPORT_USER</code> nomas iun alian. La dosierujo estas trarigardata
ĉiujn 60 sekundojn; <code>LEGEJO_IMPORT_INTERVAL</code> ŝanĝas tion, kaj
<code>LEGEJO_IMPORT_DELETE=true</code> forigas importitajn dosierojn anstataŭ movi ilin.</p>
<p>Tio estas enirejo, ne spegulo. Legejo prenas sian propran kopion kaj ne observas, kion vi faras al
la dosieroj poste.</p>"""),

 ("Konservi PDF-ojn kaj bildstriojn",
  "<p>Ne ĉio, kion vi legas, estas EPUB.</p>",
  """<p>Alŝutu PDF- kaj CBZ-dosierojn same. Ili staras apud la libroj, kun kovriloj, bretoj,
etikedoj, serĉado kaj kundividado.</p>""",
  """<p>Ili estas konservataj ekzakte tiaj, kiaj ili estas. PDF aŭ bildstrio prenas sian unuan paĝon kiel kovrilon,
kaj la <code>ComicInfo.xml</code> de bildstrio estas legata por ĝiaj titolo kaj serio. Pasvorte protektitaj
PDF-oj estas rifuzataj.</p>
<p>PDF malfermiĝas en la retumilo per <strong>Legi</strong>: la paĝoj unu post la alia, adaptitaj
al la ekrano. Zomu per la butonoj, per du fingroj aŭ per Ctrl kaj la rado; la enhavo kaj la serĉo
estas supre. Elektu tekston por marki ĝin aŭ aldoni noton, kiel en EPUB; tuŝu markon por ŝanĝi
aŭ forigi ĝin.</p>
<p>Bildstrio ankaŭ malfermiĝas per <strong>Legi</strong>: po unu paĝo adaptita al la ekrano, aŭ
du flank-al-flanke sur larĝa ekrano. Tuŝu la dekstran aŭ maldekstran flankon, ŝovu aŭ uzu la
sagoklavojn por paĝumi; tuŝu la mezon por la stangoj, kie la ŝovilo iras ien ajn en la bildstrio.
Duoble tuŝu, uzu du fingrojn aŭ Ctrl kaj la radon por zomi, kaj trenu por moviĝi sur la paĝo.
Bildstrio, kies <code>ComicInfo.xml</code> diras, ke ĝi estas mangao, estas legata de dekstre.
Bildstrio ankaŭ atingas Kobo-legilon per Kobo-sinkronigo, transformita en libron kun unu paĝo por ĉiu
bildo; PDF ne.</p>"""),

 ("Preni librojn el alia biblioteko",
  "<p>Vi volas librojn el Project Gutenberg aŭ alia biblioteko kun katalogo, sen elŝuti kaj alŝuti.</p>",
  """<p><em>Nedeviga: administranto ŝaltas ĝin por la instanco, kaj poste vi ŝaltas ĝin por
vi mem en la paĝo <strong>Konto</strong>.</em></p>
<p>Malfermu <strong>Katalogoj</strong>, aldonu katalogon per ĝia adreso aŭ elektu unu el la proponitaj,
trarigardu aŭ traserĉu ĝin, kaj prenu libron en vian bibliotekon.</p>""",
  """<p>Prenita libro estas ordinara libro: kontrolita kaj riparita kiel alŝutita, kaj ne
aldonata denove, se vi jam havas la dosieron. Legejo metas neniun permesilon sur ĝin; kion la katalogo diras
pri la rajtoj, estas montrata kun ĉiu libro, kaj kion vi rajtas fari kun ĝi, estas via afero ekscii.</p>
<p>Legejo legas OPDS&#160;1-katalogojn, kiuj estas malfermitaj al ĉiuj. Katalogoj, kiuj petas ensaluton,
kaj OPDS&#160;2 ne estas legataj.</p>"""),
 ]),

("Priskribi librojn",
 "<p>Biblioteko estas nur tiel bona kiel ĝia katalogo. Legejo legas, kion diras la dosieroj, kaj lasas vin korekti ĝin.</p>",
 [
 ("Korekti tion, kion Legejo scias pri libro",
  "<p>La titolo estas tute majuskla, la nomo de la aŭtoro estas misskribita, aŭ la lingvo estas malĝusta.</p>",
  """<p>Malfermu la libron kaj premu <strong>Redakti</strong>. Ŝanĝu <strong>Titolo</strong>,
<strong>Aŭtoro</strong>, <strong>Lingvo</strong>, <strong>Eldonejo</strong>,
<strong>Kategorio</strong>, <strong>ISBN</strong> aŭ <strong>Priskribo</strong>, kaj premu
<strong>Konservi</strong>.</p>""",
  """<p>Ĉe EPUB la korektoj estas skribataj reen en la dosieron: titolo, aŭtoroj, lingvo,
serio kaj kovrilo. Tiam la libro estas ĝusta ankaŭ en la legilo kaj en ĉiu eksporto, ne nur sur ĉi tiu
ekrano. La priskribo akceptas Markdown.</p>"""),

 ("Elserĉi libron",
  "<p>La dosiero scias preskaŭ nenion pri si mem.</p>",
  """<p><em>Laŭvole: administranto ŝaltas ĉiun serĉon sub <strong>Metadatumoj</strong> en la paĝo
Administrado.</em></p>
<p>En la redaktopaĝo, elserĉu la libron en Open Library aŭ en Libris, la nacia bibliotekkatalogo de
Svedio, kaj transprenu tion, kio taŭgas.</p>""",
  """<p>ISBN donas la plej certan trafon. Serĉo, kiu estas malŝaltita, ne havas panelon en la
redaktopaĝo.</p>"""),

 ("Teni serion en ordo",
  "<p>Vi havas sep librojn de serio kaj volas ilin en la ĝusta ordo.</p>",
  """<p>En la redaktopaĝo de ĉiu libro, skribu la saman nomon en <strong>Serio</strong> kaj la
numeron de la libro en <strong>Numero en la serio</strong>.</p>""",
  """<p>La paĝo de la libro tiam ligas al la sekva libro en la serio, kaj la serion eblas malfermi
kiel liston. En Kobo kaj en OPDS-aplikaĵoj la serio iras kune kun la libro.</p>"""),

 ("Etikedi kaj taksi",
  "<p>Vi volas propran manieron grupigi librojn, kaj memori, kion vi pensis pri ili.</p>",
  """<p>En la redaktopaĝo, skribu en <strong>Etikedoj</strong> etikedojn apartigitajn per komoj, ekzemple
<code>mystery, 1920s, favourites</code>. Taksu libron en ĝia paĝo.</p>""",
  """<p>Kategorio estas la unu larĝa speco de libro (Fikcio); etikedojn vi povas havi tiom, kiom vi volas.
Laŭ ambaŭ eblas filtri (Recepto 4.2). La taksoj aperas en la statistiko de <strong>Mia legado</strong>.</p>"""),

 ("Distingi la verkon de la eldono",
  "<p>Via ekzemplero de romano el 1879 estis eldonita en 2011, kaj la biblioteko ordigas ĝin inter la novaj libroj.</p>",
  """<p>En la redaktopaĝo, <strong>Unue eldonita (jaro)</strong> estas la jaro, kiam la verko unue
aperis. <strong>Ĉi tiu eldono</strong> estas la dato de via ekzemplero, kiel jaro aŭ kiel plena dato.</p>""",
  """<p>Se vi scias nur la jaron de la eldono kaj la libro estas freŝa, unu butono kopias ĝin al la
alia kampo.</p>"""),

 ("Ŝanĝi multajn librojn samtempe",
  "<p>Vi alŝutis kvardek librojn kaj volas etikedi ilin ĉiujn, aŭ meti ilin sur breton.</p>",
  """<p>En <strong>Mia biblioteko</strong>, premu <strong>Elekti</strong>, marku la librojn kaj elektu,
kion fari kun ili: etikedi, meti sur breton, aldoni al “Volas legi”, aŭ forigi.</p>""",
  "<p>Forigo forigas la librojn kaj iliajn dosierojn definitive, kune kun iliaj lokoj sur bretoj kaj via legopozicio en ili.</p>"),

 ("Ripari dosieron poste",
  "<p>Paĝo de libro diras <em>La dosiero bezonas atenton</em>.</p>",
  """<p>Legu la rimarkojn sub <strong>Dosierkontrolo</strong>. Se Legejo povas ripari la dosieron, premu
<strong>Ripari la dosieron</strong>. Se la rimarko temas pri io, kion nur vi povas provizi, ekzemple
kovrilo, sekvu la ligilon al la redaktopaĝo.</p>""",
  "<p>Rimarkoj, kiujn oni ne povas ripari, ekzemple ligiloj, kiuj kondukas nenien ene de la libro, restas kiel simpla informo. La libro legiĝas kiel kutime.</p>"),
 ]),

("Trovi librojn",
 "<p>Kiam la libroj ne plu eniras en unu ekranon.</p>",
 [
 ("Serĉi",
  "<p>Vi memoras titolon, aŭtoron aŭ frazon.</p>",
  """<p>Premu <code>/</code> ie ajn ekster tekstokampo, aŭ elektu <strong>Serĉi</strong>,
kaj tajpu.</p>""",
  """<p>La serĉo trairas la katalogon: titolojn, aŭtorojn, priskribojn kaj etikedojn. Ĝi trovas ankaŭ
librojn sur bretoj, kiujn aliaj kunhavigas kun vi, kaj, sub <strong>Ĉe aliaj instancoj</strong>,
librojn sur la bretoj el aliaj Legejo-instancoj, kiujn vi sekvas, ĉiu kun butono por aldoni ĝin al
via biblioteko.</p>
<p>Kie tiu, kiu funkciigas la instancon, agordis <code>LEGEJO_FULLTEXT=true</code>, marku <strong>Serĉi
en la teksto de la libroj</strong> por anstataŭe serĉi vortojn en la teksto de viaj EPUB-oj. Ĉiu libro venas kun
la trafaj fragmentoj; elektu unu por malfermi la libron kun la serĉo en ĝi. Ĉiu vorto devas aperi,
vorto trovas ankaŭ pli longajn vortojn, kiujn ĝi komencas, kaj vortoj inter citiloj devas stari unu
post la alia. Ankaŭ libroj sur kunhavigitaj bretoj estas traserĉataj kaj citataj, sed legeblaj nur
post kiam vi prenis ilin.</p>
<p>Esperanton vi povas tajpi per la x-sistemo: <code>cxambro</code> trovas <em>ĉambro</em>, en la
katalogo same kiel en la teksto.</p>"""),

 ("Filtri kaj ordigi",
  "<p>Vi volas la nelegitajn krimromanojn en la franca, la plej novajn unue.</p>",
  """<p>En <strong>Mia biblioteko</strong>, malfermu <strong>Filtri</strong> kaj elektu; la ordon vi
elektas apude. La du butonoj apud ĝi ŝanĝas inter kovriloj kaj listo.</p>""",
  "<p>La filtrilo laŭ titolo aŭ aŭtoro super la libroj malvastigas tion, kio montriĝas, dum vi tajpas.</p>"),

 ("Foliumi laŭ aŭtoro",
  "<p>Vi volas vidi ĉion de unu verkisto.</p>",
  "<p>Malfermu <strong>Aŭtoroj</strong>, aŭ alklaku la nomon de aŭtoro en iu ajn paĝo de libro.</p>",
  """<p>Libro kun pluraj aŭtoroj aperas sub ĉiu el ili.</p>
<p><em>Laŭvole: administranto ŝaltas <strong>Aŭtoroj el Vikidatumoj</strong>.</em> Tiam linio
apud la nomo de la aŭtoro diras, kiu la persono estis: la vivjarojn, kelkajn vortojn pri la persono kaj
ligilon al Wikipedia en via lingvo. Ĝi aperas nur kiam Wikidata certe konas la personon,
do verkisto kun ofta nomo, aŭ tiu, kiun Wikidata ne konas, ricevas neniun. Se la linio temas pri
iu alia, elektu <strong>Malĝusta persono?</strong>; la linio malaperas por vi, kaj
<strong>Montri denove</strong> revenigas ĝin.</p>"""),

 ("Demandi la bibliotekiston",
  "<p>Vi ne scias, kion vi volas legi, nur kian specon.</p>",
  """<p><em>Laŭvole: tiu, kiu funkciigas la instancon, agordas ĝin, kaj vi ŝaltas ĝin por vi mem en
la paĝo <strong>Konto</strong>.</em></p>
<p>Malfermu <strong>La bibliotekisto</strong> kaj demandu per viaj propraj vortoj: <em>ion mallongan kaj
amuzan, kion mi ne legis</em>, aŭ <em>kion mi legu post The Terror?</em></p>""",
  """<p>La bibliotekisto estas lingvomodelo. Al ĝi estas sendataj via demando kaj tio, kion la
katalogo diras pri la libroj: titolo, aŭtoro, bretoj, etikedoj, ĉu vi legis ilin, kaj la komenco de
la priskribo. La libroj mem neniam estas sendataj. Ĝi povas respondi nur per libroj el via biblioteko
kaj el bretoj kunhavigitaj kun vi, kaj nenio, kion ĝi skribas, estas montrata krom la elekto de libroj.
Vi povas fari 30 demandojn hore.</p>"""),
 ]),

("Bretoj kaj kunhavigo",
 "<p>Per bretoj vi ordigas bibliotekon, kaj per ili vi enlasas aliajn en parton de ĝi.</p>",
 [
 ("Fari breton",
  "<p>Vi volas vian somerlegaĵon en unu loko.</p>",
  """<p>En la redaktopaĝo de libro, sub <strong>Bretoj</strong>, tajpu nomon en
<strong>Nova breto…</strong> kaj premu <strong>Aldoni</strong>. Aŭ elektu plurajn librojn en la biblioteko
kaj metu ilin kune sur breton (Recepto 3.6).</p>""",
  """<p>Libro povas stari sur tiom da bretoj, kiom vi volas. Forigi breton forigas nur la breton; la
libroj restas en via biblioteko. En Kobo bretoj aperas kiel kolektoj.</p>"""),

 ("Kunhavigi breton",
  "<p>Vi volas, ke via familio povu legi tion, kion vi kolektis.</p>",
  """<p>Malfermu la breton, premu <strong>Redakti</strong> kaj elektu, kiu vidas ĝin: nur vi
(<em>privata</em>), la uzantoj, kiujn vi elektas (<em>limigita</em>), aŭ ĉiuj en la instanco
(<em>kunhavigita kun ĉiuj</em>).</p>""",
  """<p>Aliaj trovas ĝin sub <strong>Kunhavigitaj bretoj</strong>, kiel kovrilojn aŭ kiel liston kun la
priskribo de la breto. Donu priskribon al kunhavigita breto; ĝuste ĝi diras al homoj, ĉu indas
enrigardi.</p>"""),

 ("Preni libron de kunhavigita breto",
  "<p>Iu kunhavigis breton kun libro, kiun vi volas legi.</p>",
  "<p>Malfermu la libron sur la kunhavigita breto kaj kopiu ĝin en vian propran bibliotekon.</p>",
  """<p>La kopio estas via: via legopozicio, viaj etikedoj, viaj bretoj. Ĝi okupas neniun aldonan lokon
sur la disko, ĉar ambaŭ kopioj montras al la sama dosiero, ĝis unu el ili estas ŝanĝita.</p>"""),

 ("Sekvi kunhavigitan breton",
  "<p>Amiko daŭre aldonas librojn al breto kunhavigita kun vi, kaj vi ŝatus scii, kiam tio okazas, sen iri rigardi.</p>",
  """<p>Malfermu la breton sub <strong>Kunhavigitaj bretoj</strong> kaj premu <strong>Sekvi</strong>.
De tiam libroj, kiuj venas sur ĝin, estas anoncataj super via biblioteko, sub <strong>Nove sur
bretoj, kiujn vi sekvas</strong>, kaj ilia nombro aperas apud <strong>Kunhavigitaj bretoj</strong> en
la flanka menuo. Sur la breto mem la novaj estas markitaj per <strong>Nova</strong>.</p>
<p>Malfermi la breton validas kiel vidi ilin; <strong>Marki kiel viditan</strong> super la
biblioteko forigas ĉiujn markojn samtempe. <strong>Ĉesi sekvi</strong> troviĝas sur la breto.</p>""",
  """<p>Por breto, kiun vi sekvas, ekzistas ankaŭ <strong>Atom-fluo</strong>: adreso por fluolegilo,
kiu montras ĉiun libron, kiam ĝi alvenas. La adreso estas nur via kaj bezonas neniun ensaluton, do
gardu ĝin por vi. Ĝi ĉesas funkcii, kiam vi ĉesas sekvi, aŭ kiam la breto ne plu estas kunhavigita
kun vi. Bretoj, kiujn vi sekvas el aliaj instancoj (la sekvaj receptoj), ne estas anoncataj tiel.</p>"""),

 ("Doni breton al iu en alia instanco",
  "<p>Amiko havas propran Legejon, aŭ legas en aplikaĵo, kaj vi volas doni al tiu unu el viaj bretoj. Ne al la tuta Fediverso; nur al tiu persono.</p>",
  """<p><em>Laŭvole: administranto ŝaltas <strong>Bretoj per privata ligilo</strong> en la paĝo
Administrado.</em></p>
<p>Malfermu la breton, premu <strong>Redakti</strong> kaj iru al <strong>Privataj ligiloj</strong>. Diru,
por kiu la ligilo estas, elektu, kiom longe ĝi funkcios, kaj premu <strong>Fari ligilon</strong>. Ĝi
estas kopiata por vi; sendu ĝin al la persono.</p>""",
  """<p>La ligilo estas adreso kun longa sekreto en ĝi. Kiu ajn havas ĝin, ricevas la breton, ĝiajn
kovrilojn kaj ĝiajn librojn sen konto en via instanco kaj sen pasvorto, do sendu ĝin tiel, kiel vi
transdonus ŝlosilon, kaj faru unu por ĉiu persono. Tiam vi povas repreni unu sen ĝeni la aliajn:
premu la kruceton apud ĝi, kaj ĝi tuj ĉesas funkcii. Libroj jam prenitaj per ĝi restas, kie ili
estas.</p>
<p>La listo montras, kiam ĉiu ligilo estis laste uzita; tiel vi rimarkas ligilon, kiu trafis lokon,
kie ĝi ne devus esti. Ligilo povas ricevi lastan tagon: post semajno, monato aŭ jaro.</p>
<p>La ligilo estas ordinara OPDS-katalogo, do ĝi funkcias ankaŭ en KOReader, Thorium kaj aliaj
legaplikaĵoj, aldonita tie kiel katalogo. Viaj taksoj ne iras kune kun la libroj.</p>
<p>Ĉi tio estas por homoj, kiujn vi konas. Kion vi rajtas kunhavigi kun ili, tion juĝu vi mem.</p>"""),

 ("Sekvi breton, al kiu vi ricevis ligilon",
  "<p>Iu sendis al vi ligilon al unu el siaj bretoj.</p>",
  """<p><em>Laŭvole: la sama administra agordo kiel en la antaŭa recepto.</em></p>
<p>Malfermu <strong>Kunhavigitaj bretoj</strong>, kaj malsupre en la paĝo premu <strong>Sekvi breton
el alia Legejo-instanco</strong>. Algluu la ligilon kaj premu <strong>Sekvi</strong>. La breto malfermiĝas kun siaj libroj; premu
<strong>Aldoni al mia biblioteko</strong> ĉe tiuj, kiujn vi volas. Premu la kovrilon aŭ titolon de
libro por pli proksima rigardo: eldonejo, datoj, lingvo kaj la tuta priskribo.</p>""",
  """<p>La breto estas legata el sia ligilo, kiam vi malfermas ĝin, do ĝi estas tia, kia ĝia
posedanto havas ĝin, plus-minus kelkaj minutoj. Libro, kiun vi aldonas, estas via propra kopio, kontrolita kaj riparita kiel
alŝutita libro, kaj ĝi restas via, kio ajn okazos al la ligilo.</p>
<p>De tiam la breto estas listigita tie, sub <strong>Bretoj el aliaj Legejo-instancoj</strong>.
La listo markas breton, kies instanco ne respondas, kaj breton, kies ligilo ne plu funkcias;
tion vi vidas, kiam la posedanto reprenis ĝin. Nenio estas forigita sen via volo:
<strong>Ĉesi sekvi</strong> forprenas breton de via listo, kiam vi ne plu volas ĝin.</p>
<p>Via Legejo gardas la adreson de la ligilo por si: ĝi ne estas montrata denove, kaj kovrilojn kaj
librojn elŝutas la servilo, neniam via retumilo.</p>"""),

 ("Federacii breton de liberaj libroj",
  "<p>Vi faris belan breton de klasikaĵoj en la publika havaĵo kaj volas, ke homoj en aliaj instancoj ĝuu ĝin.</p>",
  """<p><em>Laŭvole: administranto ŝaltas federacion kaj decidas, kiuj instancoj estas permesitaj.</em></p>
<p>Faru la breton <em>federaciita</em>. Ĝi ricevas adreson laŭ la formo
<code>@shelf@your.instance</code>, kiun aliaj povas sekvi el alia Legejo, per <strong>Sekvi breton</strong>, aŭ el Mastodon.</p>""",
  """<p>Nur liberaj libroj vojaĝas tiel: en la publika havaĵo aŭ sub Creative Commons, kun indikita fonto.
Libro, kies dosiero asertas kopirajton, estas markita kiel protektita de la komenco kaj restas hejme.</p>
<p>Sekvo al aŭ de instanco, kiu ankoraŭ ne estas permesita, atendas la administranton, kiu vidas ĝin
kiel peton. Sekvata breto, kies instanco ĉesas respondi, estas markita, kaj post semajno forigita.</p>
<p>En la paĝo Fediverso, premu la kovrilon aŭ titolon de libro por tio, kion ĝia instanco scias pri
ĝi: serio, kiam ĝi unue aperis, eldonejo, permesilo, la tuta priskribo, kaj ligilo al la paĝo de la
libro tie.</p>
<p>Por komenci, la paĝo Fediverso proponas kelkajn bretojn sekvindajn, inter ili kvin Esperantajn
klasikaĵojn, kaj la langeto pri federacio en la paĝo Administrado proponas ilian instancon apud
<strong>Konekti instancon</strong>. <code>LEGEJO_SHELF_SUGGESTIONS=off</code> kaŝas ilin.</p>"""),
 ]),

("Legado en la retumilo",
 "<p>La retlegilo malfermas EPUB-librojn. Ĉio en ĉi tiu ĉapitro okazas tie.</p>",
 [
 ("Malfermi libron kaj turni paĝojn",
  "<p>Vi volas legi.</p>",
  """<p>Malfermu la libron kaj premu <strong>Legi</strong>, aŭ <strong>Daŭrigi la legadon</strong>, se
vi jam komencis. Alklaku aŭ tuŝu la dekstran trionon de la paĝo por iri antaŭen kaj la maldekstran
por iri reen. La mezo montras aŭ kaŝas la ilobretojn. Ankaŭ ŝovo per fingro turnas paĝojn, same
kiel la sagoklavoj, la spacoklavo kaj Page Down.</p>""",
  """<p>Via loko estas konservata dum vi legas kaj sendata al la servilo, do la libro malfermiĝas
tie, kie vi haltis, en ĉiu aparato. Se vi laste legis ĝin sur Kobo aŭ en KOReader, la retlegilo
komencas proksimume ĉe la sama loko: tiuj aparatoj raportas procenton, ne linion. La alia direkto
estas preciza: Kobo malfermas la libron ĉe la frazo, ĝis kiu vi venis en la retumilo, post sia
sekva sinkronigo (Recepto 8.2).</p>"""),

 ("Adapti la paĝon al vi",
  "<p>La teksto estas tro malgranda, la linioj tro densaj, aŭ la blanko tro hela.</p>",
  """<p>Malfermu <strong>Legagordoj</strong> (la butono <strong>Aa</strong>, aŭ la klavo
<code>S</code>). Tie vi elektas la etoson (<strong>Etoso</strong>: Hela, Sepia aŭ Malhela) kaj
agordas <strong>Tekstgrandeco</strong>, <strong>Tiparo</strong> (Propra de la libro, Serifa aŭ
Senserifa), <strong>Linidistanco</strong>, <strong>Marĝenoj</strong> kaj <strong>Ĝisrandigo</strong>.</p>""",
  """<p><strong>Vortodivido</strong> dividas longajn vortojn ĉe la fino de linio, laŭ la lingvo,
kiun la libro indikas; jen unu kialo, kial Recepto 2.2 zorgas pri lingvoj. <strong>Paĝa animacio</strong>
turnas la paĝon kiel folion de libro: en telefono ĝi sekvas vian fingron, kiam vi tiras la paĝon
flanken, kaj du paĝoj apude estas tirataj for kiel folio. La klavoj <code>+</code> kaj <code>−</code> ŝanĝas
la tekstgrandecon sen malfermi la panelon. La agordoj restas en la aparato, en kiu vi faris ilin.</p>"""),

 ("Trovi la vojon en libro",
  "<p>Vi serĉas la naŭan ĉapitron, aŭ la lokon, kie nomo unuafoje aperas.</p>",
  """<p>La <strong>Enhavtabelo</strong> (la klavo <code>T</code>) listigas la ĉapitrojn.
<strong>Serĉi en la libro</strong> (la klavo <code>/</code>) trovas vorton aŭ frazon kaj kondukas vin
al ĉiu loko, kie ĝi troviĝas.</p>""",
  "<p>La linio malsupre montras, kie vi estas en la ĉapitro kaj en la libro; trenu ĝin por moviĝi.</p>"),

 ("Aŭskulti libron voĉlegata",
  "<p>Viaj okuloj estas lacaj, aŭ viaj manoj okupitaj.</p>",
  """<p>Premu <strong>Voĉlegado</strong> (la klavo <code>L</code>). La legata frazo estas
markita, kaj la paĝoj turniĝas kune. Iru unu frazon reen aŭ antaŭen, ŝanĝu la rapidon
(<strong>Rapido</strong>) kaj elektu voĉon (<strong>Voĉo</strong>) inter tiuj, kiujn via aparato havas.</p>""",
  """<p>La voĉoj estas tiuj de via aparato; nenio estas sendata ien. Se la aparato ne havas voĉon
por la lingvo de la libro, legas alia voĉo, kaj diras tion. Denove la lingvo de la libro gravas.</p>"""),

 ("Marki kaj noti",
  "<p>Vi volas konservi fragmenton, kaj tion, kion vi pensis pri ĝi.</p>",
  """<p>Elektu la fragmenton. Strio aperas malsupre: premu <strong>Marki</strong>, aŭ
<strong>Aldoni noton</strong>, skribu kaj premu <strong>Konservi</strong>.</p>
<p>Kie teksto ne elekteblas, tio estas en Safari kaj en Legejo kiel aplikaĵo sur iPhone, iPad kaj Mac:
trenu la muson trans la fragmenton, aŭ tenu fingron sur vorto. Trenu plu sen levi la fingron,
aŭ poste trenu la du tenilojn, por preni pli da vortoj; <strong>Tuta frazo</strong> prenas la
frazon ĉirkaŭ ili.</p>""",
  """<p>Tuŝo sur marko malfermas ĝin, por legi la noton, ŝanĝi aŭ forigi ĝin. Per muso, se vi
tenas la montrilon sur marko, ĝia noto aperas apude. La paĝo de la libro diras, kiom da markoj
kaj notoj libro havas.</p>"""),

 ("Trarigardi viajn notojn",
  "<p>Vi finlegis la libron kaj volas tion, kion vi markis.</p>",
  """<p>Malfermu <strong>Markoj kaj notoj</strong> (la klavo <code>N</code>). Ĉiu ero montras
la fragmenton kaj la noton; alklako kondukas vin tien. La elŝuta butono supre en la panelo
donas ilin ĉiujn kiel Markdown-dosieron, same kiel <strong>Eksporti kiel Markdown</strong> sur la
paĝo de la libro.</p>
<p>Por ĉiuj libroj samtempe, malfermu <strong>Miaj notoj</strong> en la flanka menuo. La markoj
staras libro post libro, en la ordo, en kiu ili venas en la teksto, ĉiu kun sia ĉapitro kaj kiom
antaŭen en la libro ĝi troviĝas, kaj kun la paĝo de la presita eldono, kiam la libro havas tian.
Serĉu en ili, skribu aŭ ŝanĝu noton rekte tie, aŭ elektu <strong>Montri en la libro</strong> por
malfermi la retlegilon ĉe la fragmento; via loko en la libro restas, kie ĝi estis.
<strong>Eksporti ĉion</strong> donas la notojn de ĉiuj libroj en unu dosiero.</p>""",
  "<p>Markdown malfermiĝas en ĉiu notaplikaĵo kaj ĉiu tekstoredaktilo. Notoj faritaj sur Kobo ankaŭ estas en la listo, markitaj Kobo.</p>"),

 ("Legi per la klavaro",
  "<p>Vi preferas klavojn al la muso.</p>",
  """<p>Premu <code>?</code> por la listo. Resume:</p>
<table>
<tr><th>Klavo</th><th>Kion ĝi faras</th></tr>
<tr><td><code>→</code>, <code>Space</code>, <code>PgDn</code></td><td>Sekva paĝo</td></tr>
<tr><td><code>←</code>, <code>Shift</code>+<code>Space</code>, <code>PgUp</code></td><td>Antaŭa paĝo</td></tr>
<tr><td><code>T</code></td><td>Enhavtabelo</td></tr>
<tr><td><code>/</code></td><td>Serĉi en la libro</td></tr>
<tr><td><code>S</code></td><td>Legagordoj</td></tr>
<tr><td><code>N</code></td><td>Markoj kaj notoj</td></tr>
<tr><td><code>L</code></td><td>Voĉlegi, paŭzi</td></tr>
<tr><td><code>+</code>, <code>−</code></td><td>Pli granda aŭ pli malgranda teksto</td></tr>
<tr><td><code>F</code></td><td>Plenekrane</td></tr>
<tr><td><code>Esc</code></td><td>Fermi panelon; montri aŭ kaŝi la ilobretojn</td></tr>
<tr><td><code>?</code></td><td>Ĉi tiu listo</td></tr>
</table>""",
  None),

 ("Serĉi vorton en vortaro",
  "<p>Vi legas en lingvo, kiun vi ankoraŭ lernas, kaj vorto haltigas vin.</p>",
  """<p><em>Laŭvole: administranto ŝaltas <strong>Serĉi vortojn dum legado</strong> en la paĝo
Administrado.</em></p>
<p>Elektu la vorton kaj premu <strong>Serĉi en vortaro</strong>. En libro en Esperanto la vorto venas
el Reta Vortaro, la libera vortaro de Esperanto, kun tio, kio ĝi estas en via propra lingvo kaj en la
angla; en aliaj lingvoj ĝi venas el Vikivortaro, kun tio, kion ĝi signifas. La kapvorto ligas al la
tuta artikolo.</p>""",
  """<p>Vorto estas serĉata tiel, kiel la vortaro havas ĝin: <em>ĉambrojn</em> kiel <em>ĉambro</em>,
<em>legis</em> kiel <em>legi</em>. La servilo demandas la vortaron anstataŭ vi, do la vortoj, kiujn vi
serĉas, forlasas la instancon, sed ne kiu vi estas.</p>"""),
 ]),

("Legejo kiel aplikaĵo",
 """<p>Legejo povas esti metita sur la hejmekranon de telefono aŭ tabulkomputilo, aŭ en la Dock
de Mac, kie ĝi malfermiĝas en propra fenestro. Tie libroj konserveblas por legado sen reto.</p>""",
 [
 ("Meti Legejon sur la hejmekranon",
  "<p>Vi volas Legejon inter viaj aplikaĵoj, ne en retumila langeto.</p>",
  """<p><strong>iPhone kaj iPad:</strong> malfermu Legejon en Safari, premu la kunhavigan butonon
kaj elektu <strong>Aldoni al hejmekrano</strong>.</p>
<p><strong>Mac:</strong> en Safari, <strong>Dosiero</strong>, poste <strong>Aldoni al Dock</strong>.</p>
<p><strong>Android, kaj Chrome aŭ Edge en komputilo:</strong> elektu <strong>Instali</strong>
en la menuo de la retumilo.</p>""",
  """<p>Ĝi estas la sama Legejo, el la sama servilo; nenio venas el aplikaĵvendejo. Necesas, ke
la instanco estu servata per <code>https</code> (Recepto 1.4).</p>
<p>En telefono, la langetoj ĉe la malsupro de la ekrano kondukas al la biblioteko, via legado kaj la serĉo; <strong>Bretoj</strong> kaj <strong>Pli</strong> malfermas la ceteron de malsupre.</p>"""),

 ("Konservi libron en la aparato",
  "<p>Vi baldaŭ eniros aviadilon.</p>",
  """<p>Malfermu la libron kaj premu <strong>Konservi en la aparato</strong>. Kiam ĝi diras
<strong>En la aparato</strong>, la libro estas tie. Premu tiun butonon denove por forigi ĝin.</p>""",
  """<p>Sur iPhone, iPad, kaj en Safari sur Mac, la butono estas proponata nur kiam Legejo estas
sur la hejmekrano aŭ en la Dock. En retumila langeto Safari malplenigas tion, kion retejo konservis,
post semajno sen vizito, kaj libro, kiu silente malaperas, estas pli malbona ol neniu libro.
Anstataŭ proponi la butonon, la paĝo diras tion al vi.</p>
<p><strong>En ĉi tiu aparato</strong> en la menuo listigas, kio estas konservita kaj kiom da loko
ĝi okupas. Elsaluto forigas ĉion.</p>"""),

 ("Legi sen reto",
  "<p>Vi sidas en tiu aviadilo.</p>",
  """<p>Malfermu Legejon. Ĝi startas el tio, kio estas en la aparato, kaj kondukas vin al
<strong>En ĉi tiu aparato</strong>, kie estas la konservitaj libroj. Malfermu unu kaj legu.</p>""",
  """<p>Via loko estas konservata en la aparato kaj sendata al la servilo, kiam ĝi denove
atingeblas. Se vi intertempe legis pli en la sama libro en alia aparato, la pli posta loko venkas.</p>
<p>Sen reto vi povas legi viajn pli fruajn markojn sed ne fari novajn, kaj la cetero de la
biblioteko ne foliumeblas. <strong>Reprovi</strong> denove demandas la servilon.</p>"""),

 ("Kiam nova versio pretas",
  "<p>Malgranda sciigo diras <em>Nova versio de Legejo pretas</em>.</p>",
  "<p>Premu <strong>Reŝargi</strong>. Jen ĉio.</p>",
  """<p>Aplikaĵo sur la hejmekrano ne havas propran reŝargan butonon, tial Legejo proponas unu.
La sciigo aperas, kiam la servilo estis ĝisdatigita dum vi havis la aplikaĵon malfermita.</p>"""),
 ]),

("Legiloj kaj legaplikaĵoj",
 "<p>Legejo estas farita por legiloj. Sinkronigo kun ili estas parto de ĝi, ne aldonaĵo.</p>",
 [
 ("Konekti Kobo-legilon",
  "<p>Vi volas vian bibliotekon sur via Kobo, sen kabloj, ĉiufoje kiam ĝi sinkroniĝas.</p>",
  """<ol>
<li>Sur la paĝo <strong>Konto</strong>, sub <strong>Kobo-sinkronigo</strong>, premu
<strong>Krei ĵetonon</strong>, poste <strong>Kopii adreson</strong>.</li>
<li>Konektu la Kobon al komputilo per ĝia kablo.</li>
<li>Malfermu la dosieron <code>.kobo/Kobo/Kobo eReader.conf</code> en la aparato per tekstoredaktilo.
La dosierujo <code>.kobo</code> estas kaŝita.</li>
<li>Sub <code>[OneStoreServices]</code>, agordu <code>api_endpoint=</code> al la adreso, kiun vi
kopiis.</li>
<li>Elĵetu la Kobon kaj sinkronigu ĝin.</li>
</ol>""",
  """<p>Tiam Legejo staras tie, kie staris la Kobo-vendejo: la aparato demandas Legejon, kiuj libroj
ekzistas. La adreso enhavas vian ĵetonon, kiu diras al Legejo, kies bibliotekon sendi. Tenu ĝin
por vi; <strong>Krei novan</strong> faras novan, kaj la malnova ĉesas funkcii.</p>
<p><strong>La Kobo-vendejo ne plu funkcias en la aparato</strong> dum ĝi sinkroniĝas kun Legejo,
krom se administranto ŝaltis <strong>La Kobo-vendejo apud Legejo</strong> (Recepto 8.4):
foliumado, serĉado, ofertoj kaj la dezirlisto restas malplenaj, kaj libroj aĉetitaj ĉe Kobo, aŭ
pruntitaj per Kobo Plus, ne alvenas. Libroj jam en la aparato restas, kaj via Kobo-konto restas
ensalutinta. Por denove aĉeti en la aparato, remetu <code>api_endpoint=</code> al
<code>https://storeapi.kobo.com</code>.</p>
<p>La instanco devas esti atingebla de la Kobo ĉe la adreso en
<code>LEGEJO_PUBLIC_URL</code> (Recepto 1.4).</p>
<p>Post kiam la aparato sinkroniĝis, la sama loko sur la paĝo Konto listigas ĝin sub
<strong>Viaj Kobo-aparatoj</strong>, kun la tempo, kiam ĝi laste estis vidata, kaj diras, kiel
iris la lasta sinkronigo: kio estis sendita, aŭ ke ĝi malsukcesis kaj kun kia eraro. Tie unue
rigardu, kiam io ne alvenas en la aparaton.</p>"""),

 ("Scii, kio sinkroniĝas kun Kobo",
  "<p>Vi scivolas, kio aperos, kaj kio revenos.</p>",
  """<p>Viaj EPUB-libroj alvenas, survoje konvertitaj al la propra formato de Kobo, por ke la
legilo de la aparato plene funkciu. Bretoj alvenas kiel kolektoj. Via loko en ĉiu libro iras ambaŭdirekte, ĝis la frazo: legu
sur la Kobo, kaj la retumilo malfermas la libron ĉe la linio, kie vi haltis; legu en la retumilo, kaj
la Kobo malfermas la libron ĉe tiu frazo, post kiam ĝi sinkroniĝis. La tempo, kiun la Kobo kalkulas
por la legado de libro, aperas sur la paĝo de la libro. Tie aperas ankaŭ, kiom longe libro
daŭras por legi: laŭ kutima ritmo, ĝis la retleganto aŭ la Kobo mezuris la tempon de libroj, kiujn vi finlegis, kaj
de tiam laŭ via. La retleganto diras, proksimume kiom restas de la ĉapitro kaj de la libro. Bildstrioj alvenas kiel libroj kun unu paĝo po bildo.</p>""",
  """<p>PDF-dosieroj ne sinkroniĝas. Libro validas kiel finlegita, kiam 99&#160;% estas legitaj, sur
la Kobo same kiel en la retlegilo. Loko legita en la retumilo atingas libron, kiun la Kobo elŝutis;
libro, kiu ankoraŭ atendas elŝuton, ricevas ĝin kun la elŝuto. Forigo de libro sur la Kobo forigas
ĝin el la aparato, ne el via biblioteko; ĝia paĝo en Legejo povas sendi ĝin denove al la aparato.</p>"""),

 ("Sinkronigi markojn kun Kobo",
  "<p>Vi markas sur la Kobo kaj volas tiujn fragmentojn en Legejo, kaj inverse.</p>",
  """<p><em>Nedeviga: administranto ŝaltas <strong>Markoj el Kobo-legiloj</strong> sur la
paĝo Administrado.</em></p>
<p>Sinkronigu la Kobon unufoje post la ŝalto. De tiam, kion vi markas kaj notas en la aparato,
alvenas en Legejo, metita en la libron kaj markita <em>Kobo</em> en la panelo. Kion vi markas en
la retlegilo, kaj notoj, kiujn vi ŝanĝas tie, atingas la aparaton ĉe ĝia sekva sinkronigo.
Forigo ĉe unu flanko forigas ankaŭ ĉe la alia.</p>""",
  """<p>Sciu ĉi tion antaŭ ol ŝalti ĝin:</p>
<ul>
<li>La aparato tiam sendas siajn markojn al Legejo anstataŭ al Kobo, por ĉiuj siaj libroj. Tiuj
de libroj aĉetitaj ĉe Kobo ne estas akceptataj. Se ankaŭ <strong>La Kobo-vendejo apud Legejo</strong>
estas ŝaltita (Recepto 8.4), Legejo plusendas ilin al Kobo kiel antaŭe; sen ĝi ili ne plu
atingas Kobon, kaj se via aparato enhavas tiajn librojn, lasu la agordon malŝaltita.</li>
<li>Markoj, kiuj jam estis en la aparato antaŭ ol la agordo estis ŝaltita, ne estas senditaj, krom
se vi ŝanĝas ilin en la aparato.</li>
<li>Ĝi estas provita sur unu aparato, Kobo Clara&#160;2E.</li>
</ul>"""),

 ("Konservi la Kobo-vendejon",
  "<p>Vi ankaŭ aĉetas librojn ĉe Kobo, kaj volas ilin en la aparato apud viaj propraj.</p>",
  """<p><em>Nedeviga: administranto ŝaltas <strong>La Kobo-vendejo apud Legejo</strong> sur la
paĝo Administrado.</em></p>
<p>Sinkronigu la Kobon. La vendejo denove funkcias en la aparato, kaj la libroj, kiujn vi aĉetis
aŭ pruntis per Kobo Plus, alvenas apud tiuj el Legejo, kaj Kobo konservas iliajn lokojn kaj markojn
kiel antaŭe.</p>""",
  """<p>La aparato tenas sian propran Kobo-konton. Legejo respondas pri siaj propraj libroj kaj
plusendas al Kobo tion, kio koncernas tiujn de Kobo: la servilo de Legejo parolas kun tiu de Kobo
nome de vi, per la ensaluto de la aparato. Nenio el tio estas konservata en Legejo, krom ĝis kie
la sinkronigo de Kobo venis.</p>
<p>Se Kobo ne atingeblas, la libroj de Legejo tamen sinkroniĝas, kaj tiuj de Kobo venas ĉe posta
sinkronigo. Intertempe la markoj de aĉetita libro neniam ricevas la respondon „neniuj“, do la
aparato konservas ilin.</p>
<p>Aĉetitaj libroj ne estas en via Legejo-biblioteko: ili ne legeblas en la retumilo, kaj bretoj
ne akceptas ilin.</p>"""),

 ("Legi en KOReader",
  "<p>Vi legas en KOReader, sur Onyx Boox, PocketBook, Kindle aŭ Kobo.</p>",
  """<p>Prenu la librojn per OPDS. Sur la paĝo <strong>Konto</strong>, sub
<strong>OPDS-katalogo</strong>, estas la adreso, kiun vi aldonu kiel katalogon en KOReader.
Ensalutu per via uzantnomo kaj aplikaĵa pasvorto (Recepto 11.2).</p>""",
  """<p>La katalogo listigas viajn librojn, bretojn kaj seriojn, kaj eblas serĉi en ĝi.</p>
<p>KOReader sur Kobo ankaŭ povas legi la librojn, kiujn la Kobo-sinkronigo metis en la aparaton.
Legejo konas ankaŭ tiujn dosierojn, do la legopozicio de tia libro same trovas la vojon ĉi tien
(Recepto 8.6).</p>"""),

 ("Sinkronigi vian lokon kun KOReader",
  "<p>Vi volas, ke la loko, ĝis kiu vi venis en KOReader, aperu en Legejo kaj sekvu vin inter KOReader-aparatoj.</p>",
  """<ol>
<li>Sur la paĝo <strong>Konto</strong>, sub <strong>KOReader-sinkronigo</strong>, premu
<strong>Krei sinkronigan ŝlosilon</strong>.</li>
<li>En KOReader, malfermu libron, tuŝu la ilo-piktogramon kaj elektu <strong>Progreso-sinkronigo</strong>.</li>
<li>Elektu <strong>Propra sinkroniga servilo</strong> kaj enigu la servilan adreson montritan sur
la paĝo Konto.</li>
<li>Elektu <strong>Ensaluti</strong> (ne Registriĝi) kaj enigu vian uzantnomon, kaj la sinkronigan
ŝlosilon kiel pasvorton.</li>
<li>Ŝaltu aŭtomatan sinkronigon, se vi volas, ke la loko estu sendata dum vi legas.</li>
</ol>""",
  """<p>Libroj estas rekonataj laŭ sia dosiero, do ili devas veni el Legejo, per OPDS aŭ per
elŝuto. En libro Legejo montras la lokon kiel procenton; la retlegilo tiam komencas proksimume ĉe
la sama loko, ne ĉe la ĝusta linio. PDF aŭ bildstrio iras laŭ paĝo, ambaŭdirekte: la retlegilo
malfermiĝas ĉe la paĝo, ĝis kiu vi venis en KOReader, kaj KOReader ricevas la paĝon, ĝis kiu vi
venis en la retumilo, kiam ĝi estas pli nova.</p>"""),

 ("Uzi aliajn legaplikaĵojn",
  "<p>Vi legas en Moon+ Reader, Thorium aŭ alia aplikaĵo, kiu parolas OPDS.</p>",
  """<p>Aldonu la adreson el <strong>OPDS-katalogo</strong> sur la paĝo Konto kiel katalogon en
la aplikaĵo, kaj ensalutu per via uzantnomo kaj aplikaĵa pasvorto.</p>""",
  """<p>Legejo parolas OPDS&#160;1.2. EPUB-, PDF- kaj CBZ-dosieroj ĉiuj estas disdonataj tiel,
ĝuste kiel ili estas konservitaj.</p>"""),

 ("Sendi libron al Kindle",
  "<p>Vi havas Kindle, kiu ricevas siajn librojn per retpoŝto.</p>",
  """<p>Sur la paĝo <strong>Konto</strong>, sub <strong>Sendi al Kindle</strong>, donu la
adreson de via Kindle. Ĝi finiĝas per <code>@kindle.com</code>, kaj Amazon montras ĝin en via konto.
Poste premu <strong>Sendi al Kindle</strong> sur la paĝo de libro.</p>""",
  """<p>La instanco devas havi agorditan retpoŝton (Recepto 12.2), kaj Amazon devas havi la sendan
adreson de la instanco en via listo de aprobitaj sendintoj, alie ĝi silente forĵetas la libron.
Bildstrioj ne sendeblas.</p>"""),
 ]),

("Sekvi vian legadon",
 "<p>Kion vi legas, kion vi intencas legi, kaj kion vi jam legis.</p>",
 [
 ("Vidi, kie vi estas",
  "<p>Vi legas kvar librojn samtempe kaj perdis la superrigardon.</p>",
  """<p>Malfermu <strong>Mia legado</strong>. Supre estas la libroj, kiujn vi legas, kun via
progreso, pretaj por daŭrigo.</p>""",
  "<p>Via loko venas de tie, kie vi laste legis: la retlegilo, Kobo aŭ KOReader.</p>"),

 ("Teni liston de tio, kion legi poste",
  "<p>Vi ĉiam intencas legi iun libron kaj ĉiam forgesas, kiun.</p>",
  """<p>Premu <strong>Volas legi</strong> sur la paĝo de la libro, aŭ elektu plurajn librojn en la
biblioteko kaj aldonu ilin kune. Ili estas listigitaj en <strong>Mia legado</strong>.</p>""",
  "<p>Premu la butonon denove por forigi libron el la listo.</p>"),

 ("Retrorigardi",
  "<p>Vi scivolas, kiom vi legis pasintjare, kaj kion.</p>",
  """<p>Pli malsupre en <strong>Mia legado</strong> estas <strong>Laste finlegitaj</strong> kaj
<strong>Statistiko</strong>: finlegitaj libroj po monato, aldonitaj libroj po monato, la aŭtoroj,
kiujn vi plej legis, la lingvoj kaj viaj taksoj. Kun Kobo tie estas ankaŭ la tempo, kiun ĝi
kalkulis por legado, sumigita por ĉiuj libroj.</p>""",
  """<p>Libro kalkuliĝas finlegita je 99&#160;% legita, aŭ kiam vi markas ĝin tia (la sekva
recepto). Ĝia monato estas tiu, en kiu ĝi estis laste legata aŭ markita. Nenio ĉi tie forlasas
vian instancon.</p>"""),

 ("Marki libron kiel legitan, aŭ rekomenci",
  "<p>Vi legis libron sur papero, aŭ ĝiaj lastaj paĝoj estas finnotoj, do Legejo neniam kalkulas ĝin finlegita. Aŭ vi malfermis libron erare kaj ne volas ĝin inter tiuj, kiujn vi legas.</p>",
  """<p>Sur la paĝo de la libro, premu <strong>Marki kiel legitan</strong>. De tiam la libro
kalkuliĝas finlegita, kion ajn diras la pozicio.</p>
<p>Por libro erare malfermita, premu <strong>Ĉu rekomenci</strong>; por finlegita,
<strong>Marki kiel nelegitan</strong>. Ambaŭ forgesas vian lokon en la libro kaj remetas ĝin
inter la nelegitajn.</p>""",
  """<p>Rekomenco atingas ankaŭ la aparatojn. Kobo ricevas freŝan staton ĉe sia sekva sinkronigo
kaj akceptas ĝin, ĉar ĝi estas pli nova ol la propra. KOReader konservas sian propran lokon en
la libro, ĝis vi denove malfermos la libron tie, kaj tiam ĝi resendas tiun lokon; rekomencu
ankaŭ en KOReader, se vi vere volas.</p>
<p>Marki kiel legitan ankaŭ sciigas al Kobo, ke la libro estas finlegita. Kaj ĝi funkcias ankaŭ
inverse: libro markita kiel nelegita en la Kobo estas nelegita ĉi tie post la sekva sinkronigo,
kaj loko legita en la retlegilo antaŭ tio estas forgesita. La libro mem ne estas tuŝata de io
ajn el ĉi tio.</p>"""),
 ("Kunporti vian legadon aliloken",
  "<p>Vi registras vian legadon ankaŭ ĉe StoryGraph, BookWyrm aŭ Goodreads, kaj ne volas enigi ĉion denove.</p>",
  """<p>Sur la paĝo Konto, sub <strong>Eksportu vian bibliotekon</strong>, premu <strong>Legado kiel
CSV (Goodreads)</strong>. Importu la dosieron tien, kie vi volas havi vian legadon: StoryGraph,
BookWyrm kaj Goodreads ĉiuj akceptas dosieron en la propra eksportformo de Goodreads, kaj ĉi tiu
estas tia.</p>""",
  """<p>La dosiero enhavas la librojn, kiujn vi legis, legas aŭ volas legi, kun viaj taksoj, la
tago, kiam vi finis (la tago, kiam vi markis ĝin legita, aŭ laste legis en ĝi), la tago, kiam vi
aldonis ĝin, kaj viaj bretoj. Libroj, kiujn vi ne tuŝis, estas ellasitaj. La alia servo
kongruigas librojn laŭ ISBN, titolo kaj aŭtoro, do libro sen ISBN eble bezonos vian helpon
tie.</p>"""),
 ]),

("Aŭdlibroj",
 """<p>Nedeviga aldono, kaj intence malgranda. Legejo konservas la dosierojn kaj liveras ilin
kiel podkastan fluon; ĝi ne havas propran aŭskultaplikaĵon.</p>""",
 [
 ("Ŝalti aŭdlibrojn",
  "<p>Vi volas aŭdlibrojn apud viaj libroj.</p>",
  """<p>Administranto ŝaltas <strong>Subteno de aŭdlibroj</strong> en la paĝo Administrado.
Poste <strong>Aŭdlibroj</strong> aperas en la menuo.</p>""",
  "<p>Ĝi estas defaŭlte malŝaltita, ĉar sondosieroj okupas multe pli da spaco ol libroj.</p>"),

 ("Aldoni aŭdlibron",
  "<p>Vi havas dosierujon da MP3-dosieroj, aŭ unu M4B.</p>",
  """<p>En <strong>Aŭdlibroj</strong>, alŝutu la MP3-, M4A- aŭ M4B-dosierojn, aŭ la tutan
dosierujon. Titolo, aŭtoro, voĉleganto, kovrilo kaj daŭro estas legataj el la dosieroj.</p>""",
  """<p>Ĉiu dosiero estas unu parto, ludata laŭ la ordo de la dosiernomoj. Kiu funkciigas la
instancon, povas ankaŭ agordi <code>LEGEJO_AUDIOBOOK_IMPORT_DIR</code>: ĉiu dosierujo metita
ĉe ĝia supra nivelo fariĝas unu aŭdlibro, kaj bildo apud la dosieroj fariĝas la kovrilo. La sono
neniam estas tenata en la memoro, do aŭdlibroj de pluraj gigabajtoj ne estas problemo.</p>"""),

 ("Aŭskulti",
  "<p>Vi volas aŭskulti per via telefono, kaj ke ĝi memoru, kie vi estis.</p>",
  """<p>Malfermu la aŭdlibron. Ĝi havas privatan podkastan fluon, kaj butonojn, kiuj malfermas
ĝin en Apple Podcasts, Pocket Casts, Overcast kaj Castro. Ĉiu alia podkasta aplikaĵo akceptas
la adreson de la fluo.</p>""",
  """<p>La podkasta aplikaĵo konservas vian lokon, kiel ĉe ĉiu ajn programo. Jen la tuta ideo:
vi jam havas aplikaĵon, kiu lertas pri tio.</p>"""),

 ("Kunhavigi aŭdlibron",
  "<p>La ceteraj en la hejmo volas aŭdi ĝin ankaŭ.</p>",
  "<p>Kunhavigu la aŭdlibron kun la uzantoj, kiujn vi elektas, aŭ kun ĉiuj en la instanco.</p>",
  """<p>Ĉiu aŭskultanto havas propran fluadreson. Kiam la kunhavigo finiĝas, ties adreso ĉesas
funkcii, kaj nenies alia estas tuŝita.</p>"""),
 ]),

("Via konto",
 "<p>Ĉio en ĉi tiu ĉapitro estas en la paĝo <strong>Konto</strong>, atingebla per la dentrado apud via nomo.</p>",
 [
 ("Ŝanĝi vian pasvorton, bildon kaj lingvon",
  "<p>Vi ricevis pasvorton, kiun vi ne elektis, kaj la interfaco estas en la malĝusta lingvo.</p>",
  """<p>Tajpu novan pasvorton ĉe <strong>Nova pasvorto</strong> (almenaŭ 8 signoj); via
<strong>Nuna pasvorto</strong> estas bezonata por konservi. Donu adreson ĉe <strong>Retpoŝto</strong>,
por restarigi la pasvorton. Elektu bildon ĉe <strong>Profilbildo</strong>.
Elektu lingvon ĉe <strong>Lingvo</strong>: la angla, sveda, finna, germana, franca, hispana aŭ
Esperanto.</p>""",
  "<p>Kie la instanco sendas retpoŝton, nova adreso estas uzata, kiam vi malfermas la ligilon senditan al ĝi; ĝis tiam la malnova restas, kaj ĝi estas informita pri la ŝanĝo. En la angla aŭ Esperanto, <strong>Aldoni la manlibron en ĉi tiu lingvo al mia biblioteko</strong> metas ĉi tiun libron en tiu lingvo en vian bibliotekon. La lingvo ekvalidas tuj, nur por via konto. Malgranda afero, kiun indas scii: ligilo al libro aŭ breto, kiu petas vin ensaluti, kondukas vin al tiu libro aŭ breto post la ensaluto, ne al la komenca paĝo.</p>"),

 ("Doni al ĉiu aplikaĵo propran pasvorton",
  "<p>Vi preferus ne tajpi vian veran pasvorton en legoaplikaĵon.</p>",
  """<p>Sub <strong>Aplikaĵaj pasvortoj</strong>, donu nomon, ekzemple <em>KOReader sur la Boox</em>,
kaj premu <strong>Krei</strong>. Kopiu la pasvorton tuj: ĝi montriĝas nur unufoje.</p>""",
  """<p>Streketoj kaj majuskloj en ĝi ne gravas, kio helpas, kiam oni tajpas per legilo. La
listo montras, kiam ĉiu estis laste uzita. Forigu unu, kaj la aplikaĵo, kiu uzas ĝin, ne plu povas
ensaluti, sen ke io alia estas ĝenata. Via kutima pasvorto plu funkcias ĉie.</p>"""),

 ("Lasi AI-asistanton legi vian bibliotekon",
  "<p>Vi uzas AI-asistanton kaj volas demandi ĝin pri viaj propraj libroj.</p>",
  """<p><em>Nedeviga: kiu funkciigas la instancon, agordas <code>LEGEJO_MCP=true</code>.</em></p>
<p>Kreu aplikaĵan pasvorton kaj donu ĝin al la asistanto kiel bearer-ĵetonon por la adreso
<code>/api/mcp</code> en via instanco. En Claude Code, ekzemple:</p>
<pre>claude mcp add --transport http legejo https://books.example.org/api/mcp \\
  --header "Authorization: Bearer &lt;app password&gt;"</pre>""",
  """<p>Ĉi tio estas servilo de Model Context Protocol, kaj ĝi povas nur legi. La asistanto povas
serĉi en via biblioteko, rigardi libron, viajn bretojn kaj vian legadon, legi la enhavtabelon kaj
la sekciojn de libro, serĉi ene de libro, kaj vidi la bretojn, kiujn aliaj kunhavigas kun vi. Kie
la instanco konservas la tekston de la libroj (<code>LEGEJO_FULLTEXT</code>), ĝi povas ankaŭ
serĉi en la teksto de ĉiuj samtempe. Ĝi vidas vian
bibliotekon kaj nenion alian, kaj la teksto de libro forlasas la servilon nur kiam la asistanto
petas ĝin.</p>
<p>Klientoj, kiuj rezervas la kapon <code>Authorization</code> por si mem, povas anstataŭe sendi
la aplikaĵan pasvorton en kapo <code>X-Api-Key</code>.</p>"""),

 ("Elekti, kiel Legejo aspektas",
  "<p>La grizoj ne plaĉas al vi, aŭ vi volas malhelon en taglumo, aŭ iom da movo sur la paĝo.</p>",
  """<p>En la paĝo <strong>Konto</strong>, malfermu <strong>Aspekto</strong> kaj elektu unu:
<strong>Aŭtomata</strong> (hela aŭ malhela, laŭ la agordo de via sistemo), <strong>Hela</strong>,
<strong>Malhela</strong>, aŭ <strong>Zamenhof</strong>, en la brunoj de malnova lernolibro, kun la
verda stelo de Esperanto kaj libra tiparo. Sube, <strong>Montri animaciojn</strong> lasas
menuojn, dialogojn, kovrilojn kaj paĝojn aperi milde anstataŭ ĉio tuj.</p>""",
  """<p>Ambaŭ elektoj estas konservataj en la aparato, do telefono kaj komputilo povas malsami.
Animacioj estas malŝaltitaj, ĝis vi ŝaltas ilin. La retlegilo havas proprajn etosojn, en siaj
<strong>Legagordoj</strong> (Recepto 6.2), kaj la aspekto ne ŝanĝas ĝin.</p>"""),

 ("Eksporti vian bibliotekon",
  "<p>Vi volas kopion de ĉio, aŭ vi foriras.</p>",
  """<p>En la paĝo Konto, sub <strong>Eksportu vian bibliotekon</strong>, petu eksporton. Ĝi
enhavas viajn librodosierojn, la kovrilojn, kaj la katalogon kiel CSV kaj JSON. Via konto povas
esti forigita en la sama paĝo. Kiel eksporti nur vian legadon, en formo, kiun aliaj servoj
akceptas, montras Recepto 9.5.</p>""",
  """<p>Ĉar korektoj estas skribataj reen en la EPUB-dosierojn (Recepto 3.1), la eksportitaj
libroj kunportas vian laboron. Forigo de konto definitive forigas ĝiajn librojn.</p>"""),
 ]),

("Funkciigi instancon",
 "<p>Por tiu, kiu funkciigas ĝin. Plejparto de ĉi tio estas en la paĝo <strong>Administrado</strong>; la resto estas mediovariabloj.</p>",
 [
 ("Enlasi homojn",
  "<p>Vi volas doni kontojn al via familio.</p>",
  """<p>En la paĝo Administrado, sub <strong>Uzantoj</strong>, kreu inviton kaj sendu la ligilon.
Aŭ ŝaltu <strong>Permesi al uzantoj mem registriĝi</strong>.</p>""",
  """<p>Memregistriĝo postulas agorditan retpoŝton, ĉar novaj kontoj devas konfirmi sian
retpoŝtadreson. Ĉiu uzanto povas fariĝi administranto.</p>
<p>Konto povas ankaŭ foriri: <strong>Forigi konton</strong> apud uzanto forigas ĝin kun ĝiaj libroj,
bretoj, markoj, notoj kaj legado, por ĉiam. Vian propran konton vi forigas en la kontopaĝo.</p>"""),

 ("Agordi retpoŝton",
  "<p>Invitoj, novaj pasvortoj kaj Sendi al Kindle ĉiuj bezonas sendi retpoŝton.</p>",
  """<p>Agordu <code>LEGEJO_SMTP_HOST</code> kaj <code>LEGEJO_MAIL_FROM</code>, kaj laŭbezone
<code>LEGEJO_SMTP_PORT</code> (587), <code>LEGEJO_SMTP_USER</code>,
<code>LEGEJO_SMTP_PASSWORD</code> kaj <code>LEGEJO_SMTP_TLS</code> (<code>starttls</code>,
<code>tls</code> aŭ <code>none</code>).</p>""",
  """<p>Retpoŝto estas malŝaltita, kiam la retpoŝta servilo ne estas agordita, kaj ĉio, kio
dependas de ĝi, estas kaŝita. Forgesitan pasvorton oni povas tamen ripari sen ĝi (Recepto 12.8).</p>"""),

 ("Ensaluti per komuna ensaluto",
  "<p>Via hejmo jam havas unu ensaluton por ĉio.</p>",
  """<p>Agordu <code>LEGEJO_OIDC_ISSUER</code>, <code>LEGEJO_OIDC_CLIENT_ID</code> kaj
<code>LEGEJO_OIDC_CLIENT_SECRET</code>, kaj registru
<code>&lt;LEGEJO_PUBLIC_URL&gt;/api/auth/oidc/callback</code> kiel alidirektan adreson ĉe la
provizanto.</p>""",
  """<p>Ĉiu OpenID Connect-provizanto funkcias: Pocket ID, Authentik, Keycloak, Forgejo.
ID-ĵetonoj devas esti subskribitaj per RS256. Ensaluto per pasvorto plu funkcias apude.
<code>LEGEJO_OIDC_NAME</code> estas la teksto sur la butono; <code>LEGEJO_OIDC_AUTO_CREATE=true</code>
kreas kontojn por homoj ne antaŭe viditaj; <code>LEGEJO_OIDC_ADMIN_GROUP</code> faras la membrojn
de grupo administrantoj. Iu, kiu ensalutas unuafoje, estas ligata al konto kun la sama
kontrolita retpoŝtadreso.</p>"""),

 ("Elekti, kion la instanco proponas",
  "<p>Vi volas scii, kio povas esti ŝaltata kaj malŝaltata.</p>",
  """<p>En la paĝo Administrado: la serĉoj en Libris kaj Open Library, memregistriĝo, subteno de
aŭdlibroj, serĉado de vortoj dum legado, katalogoj de aliaj bibliotekoj, markoj el Kobo-legiloj, la Kobo-vendejo apud
Legejo, bretoj per privata ligilo, kaj federacio kun sia listo de permesitaj kaj blokitaj instancoj.</p>
<p>Per mediovariablo: la bibliotekisto (<code>LEGEJO_AI_API_KEY</code>, kun
<code>LEGEJO_AI_BASE_URL</code> kaj <code>LEGEJO_AI_MODEL</code>) kaj la servilo por
AI-asistantoj (<code>LEGEJO_MCP</code>).</p>""",
  """<p>La bibliotekisto funkcias kun ĉiu provizanto, kiu havas babil-API laŭ la maniero de OpenAI,
inkluzive de modelo, kiun vi mem funkciigas. Vi pagas la petojn: demando sendas la katalogon de
la libroj de tiu uzanto, do ĝia kosto kreskas kun la biblioteko. Kelkcent libroj prenas ĉirkaŭ
dudek mil ĵetonojn. La paĝo Administrado montras, kiom da demandoj estis faritaj kaj kiom da
ĵetonoj ili prenis.</p>
<p>Sen ĉi tiuj du, nenio en Legejo parolas kun lingvomodelo.</p>"""),

 ("Vidi, kio okazis",
  "<p>Io estas stranga, kaj vi volas scii, kion la instanco faris.</p>",
  "<p>La paĝo Administrado havas sisteman protokolon de ĉio, kio okazas en la instanco: ensalutoj, alŝutoj, sinkronigoj, kunhavigoj, malsukcesoj.</p>",
  """<p>Por la propra protokolo de la servilo, <code>LEGEJO_LOG_FORMAT=json</code> donas unu
JSON-objekton po linio, kaj <code>RUST_LOG</code> agordas la nivelon.</p>"""),

 ("Mezuri",
  "<p>Vi uzas Prometheus kaj volas Legejon en ĝi.</p>",
  """<p>Agordu <code>LEGEJO_METRICS_TOKEN</code>; tiam <code>/metrics</code> respondas al petoj,
kiuj portas <code>Authorization: Bearer &lt;token&gt;</code>.</p>""",
  """<p>Ĝi raportas uzantojn, librojn, konservejon, bretojn, la federacian vicon, eksportojn,
malsukcesajn ensalutojn kaj aktivecon po speco de evento. <code>LEGEJO_METRICS=true</code>
malfermas ĝin sen ĵetono, kio taŭgas nur por privataj retoj.</p>"""),

 ("Teni pasvortodivenantojn ekstere",
  "<p>Via instanco estas en la interreto, kaj iu provas pasvortojn.</p>",
  "<p>Nenio farenda: malsukcesaj ensalutoj estas kalkulataj, en la retejo kaj per OPDS, kaj respondataj per HTTP&#160;429, tuj kiam la limo estas atingita ene de la tempofenestro.</p>",
  """<p>La limo estas 10 malsukcesoj po uzantnomo en 15 minutoj, kaj 50 po klienta adreso por
ĉiuj uzantnomoj kune. <code>LEGEJO_LOGIN_MAX_FAILURES</code>, <code>LEGEJO_LOGIN_MAX_FAILURES_IP</code> kaj
<code>LEGEJO_LOGIN_WINDOW_MINUTES</code> ŝanĝas ilin. Malantaŭ inversa prokurilo, indiku la kapon,
kiu portas la adreson de la kliento (Recepto 1.4), alie ĉiu vizitanto aspektas kiel la prokurilo.</p>"""),

 ("Helpi iun, kiu perdis sian pasvorton",
  "<p>Uzanto forgesis sian pasvorton, kaj la instanco sendas neniun retpoŝton, do la ligilo en la ensalutpaĝo nenion faras por tiu.</p>",
  """<p>En la paĝo Administrado, sub <strong>Uzantoj</strong>, premu <strong>Ligilo por nova pasvorto</strong>
apud la uzanto kaj sendu al tiu la ligilon. Ĝi funkcias unufoje, dum unu tago; per ĝi oni
elektas novan pasvorton kaj estas elsalutita ĉie aliloke.</p>
<p>Se estas la administranto, kiu perdis la pasvorton, rulu apud la
<code>docker-compose.yml</code> de la instanco:</p>
<pre>docker compose exec legejo legejo reset-password &lt;username&gt;</pre>
<p>Ĝi presas tian ligilon por iu ajn uzanto.</p>""",
  """<p>La ligilo komenciĝas per <code>LEGEJO_PUBLIC_URL</code>, kiam tiu estas agordita; alie
metu la adreson de la instanco antaŭ ĝi. Ĝis la ligilo estas uzita, la malnova pasvorto plu
funkcias. Ambaŭ manieroj estas notataj en la sistema protokolo (Recepto 12.5).</p>"""),

 ("Trovi agordon",
  "<p>Vi memoras, ke ekzistas variablo por tio, sed ne ĝian nomon.</p>",
  """<table>
<tr><th>Variablo</th><th>Defaŭlto</th><th>Kio ĝi estas</th></tr>
<tr><td><code>DATABASE_URL</code></td><td>SQLite en la datumdosierujo</td><td><code>postgres://…</code> aŭ <code>sqlite://…</code></td></tr>
<tr><td><code>LEGEJO_DATA_DIR</code></td><td><code>/data</code></td><td>Librodosieroj, kovriloj kaj la SQLite-datumbazo</td></tr>
<tr><td><code>LEGEJO_ADDR</code></td><td><code>0.0.0.0:3000</code></td><td>Adreso, ĉe kiu aŭskulti</td></tr>
<tr><td><code>LEGEJO_PUBLIC_URL</code></td><td></td><td>La publika adreso <code>https://</code></td></tr>
<tr><td><code>LEGEJO_SECURE_COOKIES</code></td><td>ŝaltita kun <code>https</code></td><td><code>false</code> por servi per simpla HTTP</td></tr>
<tr><td><code>LEGEJO_ADMIN_USER</code></td><td><code>admin</code></td><td>Unua administranto, kiam la datumbazo estas malplena</td></tr>
<tr><td><code>LEGEJO_ADMIN_PASSWORD</code></td><td>generita</td><td>Ĝia pasvorto; presita unufoje en la protokolo</td></tr>
<tr><td><code>LEGEJO_MAX_UPLOAD_MB</code></td><td><code>200</code></td><td>Plej granda akceptata alŝuto</td></tr>
<tr><td><code>LEGEJO_LOG_FORMAT</code></td><td><code>text</code></td><td><code>json</code> por unu objekto po linio</td></tr>
<tr><td><code>LEGEJO_IMPORT_DIR</code></td><td></td><td>Dosierujo observata por libroj</td></tr>
<tr><td><code>LEGEJO_AUDIOBOOK_IMPORT_DIR</code></td><td></td><td>Dosierujo observata por aŭdlibroj</td></tr>
<tr><td><code>LEGEJO_MCP</code></td><td><code>false</code></td><td>La servilo por AI-asistantoj</td></tr>
<tr><td><code>LEGEJO_FULLTEXT</code></td><td><code>false</code></td><td>Serĉado ene de la libroj</td></tr>
<tr><td><code>LEGEJO_SHELF_SUGGESTIONS</code></td><td>ŝaltita</td><td><code>off</code> kaŝas la proponitajn bretojn</td></tr>
<tr><td><code>LEGEJO_AI_API_KEY</code></td><td></td><td>Ŝaltas la bibliotekiston</td></tr>
<tr><td><code>LEGEJO_SMTP_HOST</code></td><td></td><td>Retpoŝta servilo; retpoŝto estas malŝaltita, se ĝi mankas</td></tr>
<tr><td><code>LEGEJO_OIDC_ISSUER</code></td><td></td><td>Komuna ensaluto; malŝaltita, se ĝi mankas</td></tr>
<tr><td><code>LEGEJO_METRICS_TOKEN</code></td><td></td><td>Ŝaltas <code>/metrics</code></td></tr>
</table>""",
  """<p>Ĉiuj estas nedevigaj, kaj ĉiu el ili povas esti legata el dosiero, se oni aldonas
<code>_FILE</code> al ĝia nomo. La README en la deponejo havas la plenan liston.</p>"""),
 ]),
]

APPENDIX = """
<p>Kuirlibro diru ankaŭ, por kio la kuirejo ne estas ekipita.</p>
<dl>
<dt>Legejo ne legas dosierujon da libroj surloke.</dt>
<dd>Ĝi tenas propran kopion de ĉiu libro, por povi ripari la dosieron kaj skribi viajn
korektojn reen en ĝin. Se via dosierujstrukturo devas resti la fonto de la vero, Legejo ne estas
la ĝusta ilo.</dd>
<dt>Legejo legas PDF-dosierojn kaj bildstriojn simple.</dt>
<dd>Ili malfermiĝas en la retumilo, sed sen la agordoj de la EPUB-leganto aŭ laŭtlegado, kaj PDF
ne sinkroniĝas al Kobo.</dd>
<dt>Legejo ne havas aŭskultaplikaĵon.</dt>
<dd>Aŭdlibroj estas liverataj kiel podkastaj fluoj, por la podkasta aplikaĵo, kiun vi jam uzas.</dd>
<dt>Legejo ne malfermas kopiprotektitajn librojn.</dt>
<dd>Dosieroj kun DRM estas rifuzataj ĉe alŝuto.</dd>
<dt>Legejo ne raportas hejmen.</dt>
<dd>Ne ekzistas telemetrio, nek pagata versio. La sola fojo, kiam io memstare demandas ion al la
ekstera mondo, estas kiam vi premas butonon, kiu diras, ke ĝi faros tion.</dd>
</dl>
"""

COLOPHON = """
<p>La bestoj sur la kovrilo de <em>La Kuirlibro de Legejo</em> estas rokokolomboj
(<em>Columba livia</em>), pli konataj simple kiel kolomboj. Ili estas la birdoj de la propra
emblemo de Legejo, kaj estis elektitaj por ĝi, ĉar malmultaj estaĵoj tiel lertas trovi la vojon
hejmen kun io, kion vi donis al ili por porti.</p>
<p>Rokokolomboj vivas apud homoj jam de kelkmil jaroj, unue sur klifoj kaj poste sur la
konstruaĵoj, kiujn homoj starigis, kaj kiuj por kolombo estas klifoj kun pli bona manĝaĵo. Ili
orientiĝas laŭ la suno, laŭ la magneta kampo de la tero kaj laŭ orientiloj, kaj trejnita birdo
revenos al sia kolombejo de centoj da kilometroj for. Dum la plej granda parto de la historio tio
faris ilin la plej rapida maniero sendi mesaĝon. Ili pariĝas por la tuta vivo, kune kovas la
ovojn, kaj povas lerni rekoni sin mem en spegulo, kio estas pli ol oni povas diri pri plejparto
de la programaro.</p>
<p>La kolomboj sur la kovrilo ne estas endanĝerigitaj. Tamen estu afablaj al ili.</p>
<p>La kovrilo imitas iun specon de komputila libro, el kiu multaj el ni lernis, kun amo kaj sen
permeso; ĝi estas pastiĉo, kaj ĉi tiu libro havas nenion komunan kun ties eldonejo. La nomo
<em>Legejo</em> estas, kiel ĉiu esperantisto tuj vidas, simple leg-ej-o: loko, kie oni legas.</p>
<p>La teksto estas kompostita per tiu tiparo, kiun via legilo preferas. Legejo estas libera
programaro laŭ la GNU Affero General Public License.</p>
"""
